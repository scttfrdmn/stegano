//! Analysis tools for steganography performance and detection resistance
//!
//! This module provides statistical analysis, performance evaluation,
//! and detection resistance assessment capabilities.

use super::*;
use crate::core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard normal CDF via Abramowitz & Stegun approximation 7.1.26 (max error 1.5e-7).
fn normal_cdf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t * (0.254_829_592
        + t * (-0.284_496_736
            + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let approx = 1.0 - poly * (-x.abs() * x.abs()).exp();
    if x >= 0.0 { approx } else { 1.0 - approx }
}

/// Linear-interpolation percentile on a pre-sorted slice. `p` in [0, 1].
fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = p * (sorted.len() - 1) as f32;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    sorted[lo] + (pos - lo as f32) * (sorted[hi] - sorted[lo])
}

/// Kolmogorov-Smirnov statistic of `sorted` against Normal(mean, std).
fn ks_vs_normal(sorted: &[f32], mean: f32, std: f32) -> f32 {
    let n = sorted.len() as f32;
    sorted
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let theoretical = if std > 0.0 { normal_cdf((x - mean) / std) } else { 0.5 };
            ((i + 1) as f32 / n - theoretical).abs()
        })
        .fold(0.0f32, f32::max)
}

/// Kolmogorov-Smirnov statistic of `sorted` against Exponential(rate = 1/mean).
fn ks_vs_exponential(sorted: &[f32], mean: f32) -> f32 {
    let n = sorted.len() as f32;
    let rate = if mean > 0.0 { 1.0 / mean } else { 1.0 };
    sorted
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let theoretical = if x < 0.0 { 0.0 } else { 1.0 - (-rate * x).exp() };
            ((i + 1) as f32 / n - theoretical).abs()
        })
        .fold(0.0f32, f32::max)
}

/// Kolmogorov-Smirnov statistic of `sorted` against Uniform(min, max).
fn ks_vs_uniform(sorted: &[f32], min: f32, max: f32) -> f32 {
    let n = sorted.len() as f32;
    let range = max - min;
    if range.abs() < f32::EPSILON {
        return 1.0;
    }
    sorted
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let theoretical = ((x - min) / range).clamp(0.0, 1.0);
            ((i + 1) as f32 / n - theoretical).abs()
        })
        .fold(0.0f32, f32::max)
}

/// Fill all `DescriptiveStats` fields for a single named metric.
fn record_metric(name: &str, values: &[f32], desc: &mut DescriptiveStats) {
    let n = values.len();
    if n == 0 {
        return;
    }
    let nf = n as f32;
    let mean = values.iter().sum::<f32>() / nf;
    let var = values.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / nf;
    let std = var.sqrt();

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min = sorted[0];
    let max = sorted[n - 1];
    let median = percentile(&sorted, 0.5);
    let q1 = percentile(&sorted, 0.25);
    let q3 = percentile(&sorted, 0.75);

    let skewness = if std > 0.0 {
        values.iter().map(|&x| ((x - mean) / std).powi(3)).sum::<f32>() / nf
    } else {
        0.0
    };
    let kurtosis = if std > 0.0 {
        values.iter().map(|&x| ((x - mean) / std).powi(4)).sum::<f32>() / nf - 3.0
    } else {
        0.0
    };

    let k = name.to_string();
    desc.means.insert(k.clone(), mean);
    desc.std_devs.insert(k.clone(), std);
    desc.mins.insert(k.clone(), min);
    desc.maxs.insert(k.clone(), max);
    desc.medians.insert(k.clone(), median);
    desc.quantiles.insert(k.clone(), (q1, q3));
    desc.skewness.insert(k.clone(), skewness);
    desc.kurtosis.insert(k, kurtosis);
}

/// Performance analysis engine
#[derive(Debug)]
pub struct AnalysisEngine {
    /// Statistical analyzers
    analyzers: Vec<Box<dyn StatisticalAnalyzer>>,

    /// Detection resistance evaluators
    detection_evaluators: Vec<Box<dyn DetectionEvaluator>>,

    /// Performance benchmarks
    benchmarks: Vec<PerformanceBenchmark>,

    /// Analysis configuration
    config: AnalysisConfig,
}

/// Configuration for analysis operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Enable statistical analysis
    pub statistical_analysis: bool,

    /// Enable detection resistance testing
    pub detection_testing: bool,

    /// Confidence level for statistical tests
    pub confidence_level: f32,

    /// Number of bootstrap samples
    pub bootstrap_samples: usize,

    /// Enable cross-format analysis
    pub cross_format_analysis: bool,

    /// Performance comparison thresholds
    pub performance_thresholds: PerformanceThresholds,
}

/// Performance threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    /// Minimum acceptable capacity efficiency
    pub min_capacity_efficiency: f32,

    /// Maximum acceptable embedding time (ms)
    pub max_embedding_time_ms: u64,

    /// Minimum acceptable extraction accuracy
    pub min_extraction_accuracy: f32,

    /// Minimum acceptable quality preservation
    pub min_quality_preservation: f32,

    /// Minimum acceptable detection resistance
    pub min_detection_resistance: f32,
}

/// Statistical analyzer trait
pub trait StatisticalAnalyzer: std::fmt::Debug + Send + Sync {
    /// Perform statistical analysis on performance data
    fn analyze(&self, data: &[PerformanceMetrics]) -> Result<StatisticalSummary>;

    /// Get analyzer name
    fn name(&self) -> &str;

    /// Get required minimum sample size
    fn min_sample_size(&self) -> usize;
}

/// Performance distribution analyzer
#[derive(Debug)]
pub struct DistributionAnalyzer {
    /// Distribution type to test for
    _distribution_tests: Vec<DistributionTest>,
}

/// Distribution test types
#[derive(Debug, Clone)]
pub enum DistributionTest {
    Normality,
    Exponential,
    Uniform,
    LogNormal,
}

/// Correlation analyzer
#[derive(Debug)]
pub struct CorrelationAnalyzer {
    /// Correlation method
    method: CorrelationMethod,
}

/// Correlation analysis methods
#[derive(Debug, Clone)]
pub enum CorrelationMethod {
    Pearson,
    Spearman,
    Kendall,
}

/// Outlier detection analyzer
#[derive(Debug)]
pub struct OutlierAnalyzer {
    /// Detection method
    method: OutlierMethod,

    /// Sensitivity threshold
    threshold: f32,
}

/// Outlier detection methods
#[derive(Debug, Clone)]
pub enum OutlierMethod {
    IQR,
    ZScore,
    ModifiedZScore,
    IsolationForest,
}

/// Statistical summary result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalSummary {
    /// Basic descriptive statistics
    pub descriptive: DescriptiveStats,

    /// Distribution analysis results
    pub distribution: DistributionAnalysis,

    /// Correlation analysis results
    pub correlations: HashMap<String, f32>,

    /// Outlier detection results
    pub outliers: OutlierAnalysis,

    /// Confidence intervals
    pub confidence_intervals: HashMap<String, (f32, f32)>,

    /// Statistical test results
    pub test_results: Vec<StatisticalTest>,
}

/// Descriptive statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptiveStats {
    /// Sample size
    pub count: usize,

    /// Mean values
    pub means: HashMap<String, f32>,

    /// Standard deviations
    pub std_devs: HashMap<String, f32>,

    /// Minimum values
    pub mins: HashMap<String, f32>,

    /// Maximum values
    pub maxs: HashMap<String, f32>,

    /// Median values
    pub medians: HashMap<String, f32>,

    /// Quantiles (25%, 75%)
    pub quantiles: HashMap<String, (f32, f32)>,

    /// Skewness measures
    pub skewness: HashMap<String, f32>,

    /// Kurtosis measures
    pub kurtosis: HashMap<String, f32>,
}

/// Distribution analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionAnalysis {
    /// Best fitting distribution
    pub best_fit: String,

    /// Distribution parameters
    pub parameters: HashMap<String, f32>,

    /// Goodness of fit scores
    pub goodness_of_fit: HashMap<String, f32>,

    /// Distribution test p-values
    pub test_p_values: HashMap<String, f32>,
}

/// Outlier analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlierAnalysis {
    /// Detected outlier indices
    pub outlier_indices: Vec<usize>,

    /// Outlier scores
    pub outlier_scores: Vec<f32>,

    /// Threshold used for detection
    pub threshold: f32,

    /// Method used
    pub method: String,
}

/// Statistical test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalTest {
    /// Test name
    pub name: String,

    /// Test statistic
    pub statistic: f32,

    /// P-value
    pub p_value: f32,

    /// Critical value
    pub critical_value: f32,

    /// Test conclusion
    pub conclusion: String,
}

/// Detection resistance evaluator trait
pub trait DetectionEvaluator: std::fmt::Debug + Send + Sync {
    /// Evaluate detection resistance of embedded data
    fn evaluate(&self, original: &[u8], stego: &[u8]) -> Result<DetectionScore>;

    /// Get evaluator name
    fn name(&self) -> &str;

    /// Get detection method category
    fn category(&self) -> DetectionCategory;
}

/// Detection analysis categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionCategory {
    Statistical,
    Visual,
    Structural,
    MachineLearning,
    Frequency,
}

/// Chi-square statistical test for detection
#[derive(Debug)]
pub struct ChiSquareDetector {
    /// Significance level
    _alpha: f32,
}

/// Sample Pairs (SP) analysis
#[derive(Debug)]
pub struct SamplePairsDetector;

/// Regular Singular (RS) analysis
#[derive(Debug)]
pub struct RegularSingularDetector;

/// Weighted Stego (WS) detector
#[derive(Debug)]
pub struct WeightedStegoDetector;

/// Visual detection through histograms
#[derive(Debug)]
pub struct HistogramDetector;

/// Machine learning-based detector
#[derive(Debug)]
pub struct MLDetector {
    /// Trained classifier model
    _model_path: String,
}

/// Detection score result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionScore {
    /// Detection probability (0.0 = undetectable, 1.0 = easily detected)
    pub detection_probability: f32,

    /// Confidence in the detection
    pub confidence: f32,

    /// Detailed scores by method
    pub method_scores: HashMap<String, f32>,

    /// Evidence of embedding
    pub evidence: Vec<DetectionEvidence>,

    /// Overall resistance grade
    pub resistance_grade: ResistanceGrade,
}

/// Detection evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionEvidence {
    /// Evidence description
    pub description: String,

    /// Strength of evidence (0.0-1.0)
    pub strength: f32,

    /// Detection method that found this evidence
    pub method: String,

    /// Location/region where evidence was found
    pub location: Option<String>,
}

/// Resistance grade classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResistanceGrade {
    Excellent, // Very hard to detect
    Good,      // Moderately hard to detect
    Fair,      // Some detectable patterns
    Poor,      // Easily detectable
    Failed,    // Detection is trivial
}

/// Performance benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBenchmark {
    /// Benchmark name
    pub name: String,

    /// Data format
    pub format: DataFormat,

    /// Payload size
    pub payload_size: usize,

    /// Carrier size
    pub carrier_size: usize,

    /// Reference performance metrics
    pub reference_metrics: PerformanceMetrics,

    /// Detection resistance score
    pub reference_detection_score: f32,

    /// Test conditions
    pub conditions: HashMap<String, String>,
}

/// Comprehensive analysis result
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Statistical analysis summary
    pub statistical_summary: StatisticalSummary,

    /// Detection resistance scores
    pub detection_scores: Vec<DetectionScore>,

    /// Performance comparison with benchmarks
    pub benchmark_comparison: BenchmarkComparison,

    /// Recommendations for improvement
    pub recommendations: Vec<PerformanceRecommendation>,

    /// Analysis metadata
    pub metadata: AnalysisMetadata,
}

/// Benchmark comparison results
#[derive(Debug, Clone)]
pub struct BenchmarkComparison {
    /// Benchmark scores
    pub scores: HashMap<String, f32>,

    /// Performance ranking
    pub ranking: HashMap<String, usize>,

    /// Significant differences
    pub significant_differences: Vec<String>,

    /// Overall assessment
    pub overall_assessment: String,
}

/// Performance improvement recommendation
#[derive(Debug, Clone)]
pub struct PerformanceRecommendation {
    /// Recommendation category
    pub category: RecommendationCategory,

    /// Description
    pub description: String,

    /// Expected improvement
    pub expected_improvement: f32,

    /// Implementation difficulty
    pub difficulty: DifficultyLevel,

    /// Priority
    pub priority: PriorityLevel,
}

/// Recommendation categories
#[derive(Debug, Clone)]
pub enum RecommendationCategory {
    Algorithm,
    Parameters,
    Preprocessing,
    Postprocessing,
    HardwareOptimization,
}

/// Implementation difficulty levels
#[derive(Debug, Clone)]
pub enum DifficultyLevel {
    Easy,
    Medium,
    Hard,
    Expert,
}

/// Priority levels
#[derive(Debug, Clone)]
pub enum PriorityLevel {
    Critical,
    High,
    Medium,
    Low,
}

/// Analysis metadata
#[derive(Debug, Clone)]
pub struct AnalysisMetadata {
    /// Analysis timestamp
    pub timestamp: u64,

    /// Analysis duration
    pub duration_ms: u64,

    /// Sample size
    pub sample_size: usize,

    /// Analysis version
    pub version: String,

    /// Configuration used
    pub config: AnalysisConfig,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            statistical_analysis: true,
            detection_testing: true,
            confidence_level: 0.95,
            bootstrap_samples: 1000,
            cross_format_analysis: false,
            performance_thresholds: PerformanceThresholds {
                min_capacity_efficiency: 0.5,
                max_embedding_time_ms: 1000,
                min_extraction_accuracy: 0.95,
                min_quality_preservation: 0.8,
                min_detection_resistance: 0.7,
            },
        }
    }
}

impl AnalysisEngine {
    /// Create new analysis engine
    pub fn new(config: AnalysisConfig) -> Self {
        let mut engine = Self {
            analyzers: Vec::new(),
            detection_evaluators: Vec::new(),
            benchmarks: Vec::new(),
            config,
        };

        // Add default analyzers
        engine.add_analyzer(Box::new(DistributionAnalyzer::new()));
        engine.add_analyzer(Box::new(CorrelationAnalyzer::new(
            CorrelationMethod::Pearson,
        )));
        engine.add_analyzer(Box::new(OutlierAnalyzer::new(OutlierMethod::IQR, 1.5)));

        // Add default detection evaluators
        engine.add_detection_evaluator(Box::new(ChiSquareDetector::new(0.05)));
        engine.add_detection_evaluator(Box::new(SamplePairsDetector));
        engine.add_detection_evaluator(Box::new(HistogramDetector));

        engine
    }

    /// Add statistical analyzer
    pub fn add_analyzer(&mut self, analyzer: Box<dyn StatisticalAnalyzer>) {
        self.analyzers.push(analyzer);
    }

    /// Add detection evaluator
    pub fn add_detection_evaluator(&mut self, evaluator: Box<dyn DetectionEvaluator>) {
        self.detection_evaluators.push(evaluator);
    }

    /// Add performance benchmark
    pub fn add_benchmark(&mut self, benchmark: PerformanceBenchmark) {
        self.benchmarks.push(benchmark);
    }

    /// Perform comprehensive analysis
    pub async fn analyze(
        &self,
        performance_data: &[PerformanceMetrics],
        stego_samples: &[(Vec<u8>, Vec<u8>)], // (original, stego) pairs
    ) -> Result<AnalysisResult> {
        let start_time = std::time::Instant::now();

        // Statistical analysis
        let statistical_summary = if self.config.statistical_analysis {
            self.perform_statistical_analysis(performance_data).await?
        } else {
            StatisticalSummary {
                descriptive: DescriptiveStats {
                    count: 0,
                    means: HashMap::new(),
                    std_devs: HashMap::new(),
                    mins: HashMap::new(),
                    maxs: HashMap::new(),
                    medians: HashMap::new(),
                    quantiles: HashMap::new(),
                    skewness: HashMap::new(),
                    kurtosis: HashMap::new(),
                },
                distribution: DistributionAnalysis {
                    best_fit: "unknown".to_string(),
                    parameters: HashMap::new(),
                    goodness_of_fit: HashMap::new(),
                    test_p_values: HashMap::new(),
                },
                correlations: HashMap::new(),
                outliers: OutlierAnalysis {
                    outlier_indices: Vec::new(),
                    outlier_scores: Vec::new(),
                    threshold: 0.0,
                    method: "none".to_string(),
                },
                confidence_intervals: HashMap::new(),
                test_results: Vec::new(),
            }
        };

        // Detection resistance analysis
        let detection_scores = if self.config.detection_testing {
            self.perform_detection_analysis(stego_samples).await?
        } else {
            Vec::new()
        };

        // Benchmark comparison
        let benchmark_comparison = self.compare_with_benchmarks(performance_data).await?;

        // Generate recommendations
        let recommendations = self.generate_recommendations(
            &statistical_summary,
            &detection_scores,
            &benchmark_comparison,
        );

        let duration = start_time.elapsed().as_millis() as u64;

        Ok(AnalysisResult {
            statistical_summary,
            detection_scores,
            benchmark_comparison,
            recommendations,
            metadata: AnalysisMetadata {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                duration_ms: duration,
                sample_size: performance_data.len(),
                version: "1.0.0".to_string(),
                config: self.config.clone(),
            },
        })
    }

    /// Perform statistical analysis
    async fn perform_statistical_analysis(
        &self,
        performance_data: &[PerformanceMetrics],
    ) -> Result<StatisticalSummary> {
        if performance_data.is_empty() {
            return Err(SteganoError::InvalidParameter(
                "No performance data provided".to_string(),
            ));
        }

        // Run all analyzers
        let mut combined_summary = StatisticalSummary {
            descriptive: self.calculate_descriptive_stats(performance_data),
            distribution: DistributionAnalysis {
                best_fit: "normal".to_string(),
                parameters: HashMap::new(),
                goodness_of_fit: HashMap::new(),
                test_p_values: HashMap::new(),
            },
            correlations: HashMap::new(),
            outliers: OutlierAnalysis {
                outlier_indices: Vec::new(),
                outlier_scores: Vec::new(),
                threshold: 0.0,
                method: "none".to_string(),
            },
            confidence_intervals: HashMap::new(),
            test_results: Vec::new(),
        };

        for analyzer in &self.analyzers {
            if performance_data.len() >= analyzer.min_sample_size() {
                let summary = analyzer.analyze(performance_data)?;
                // Merge results (simplified)
                combined_summary.correlations.extend(summary.correlations);
                combined_summary.test_results.extend(summary.test_results);
            }
        }

        Ok(combined_summary)
    }

    /// Perform detection resistance analysis
    async fn perform_detection_analysis(
        &self,
        stego_samples: &[(Vec<u8>, Vec<u8>)],
    ) -> Result<Vec<DetectionScore>> {
        let mut detection_scores = Vec::new();

        for (original, stego) in stego_samples {
            let mut combined_score = DetectionScore {
                detection_probability: 0.0,
                confidence: 0.0,
                method_scores: HashMap::new(),
                evidence: Vec::new(),
                resistance_grade: ResistanceGrade::Good,
            };

            for evaluator in &self.detection_evaluators {
                let score = evaluator.evaluate(original, stego)?;
                combined_score
                    .method_scores
                    .insert(evaluator.name().to_string(), score.detection_probability);
                combined_score.evidence.extend(score.evidence);
            }

            // Calculate overall detection probability
            if !combined_score.method_scores.is_empty() {
                combined_score.detection_probability =
                    combined_score.method_scores.values().sum::<f32>()
                        / combined_score.method_scores.len() as f32;
            }

            // Assign resistance grade
            combined_score.resistance_grade = match combined_score.detection_probability {
                p if p < 0.2 => ResistanceGrade::Excellent,
                p if p < 0.4 => ResistanceGrade::Good,
                p if p < 0.6 => ResistanceGrade::Fair,
                p if p < 0.8 => ResistanceGrade::Poor,
                _ => ResistanceGrade::Failed,
            };

            detection_scores.push(combined_score);
        }

        Ok(detection_scores)
    }

    /// Compare performance with benchmarks
    async fn compare_with_benchmarks(
        &self,
        performance_data: &[PerformanceMetrics],
    ) -> Result<BenchmarkComparison> {
        let mut scores = HashMap::new();
        let ranking = HashMap::new();

        // Calculate average performance
        if !performance_data.is_empty() {
            let avg_capacity = performance_data
                .iter()
                .map(|p| p.capacity_efficiency)
                .sum::<f32>()
                / performance_data.len() as f32;
            let avg_time = performance_data
                .iter()
                .map(|p| p.embedding_time_ms)
                .sum::<u64>()
                / performance_data.len() as u64;

            scores.insert("capacity_efficiency".to_string(), avg_capacity);
            scores.insert("embedding_time_ms".to_string(), avg_time as f32);
        }

        Ok(BenchmarkComparison {
            scores,
            ranking,
            significant_differences: Vec::new(),
            overall_assessment: "Analysis completed".to_string(),
        })
    }

    /// Generate performance recommendations
    fn generate_recommendations(
        &self,
        _statistical_summary: &StatisticalSummary,
        _detection_scores: &[DetectionScore],
        _benchmark_comparison: &BenchmarkComparison,
    ) -> Vec<PerformanceRecommendation> {
        vec![
            PerformanceRecommendation {
                category: RecommendationCategory::Algorithm,
                description:
                    "Consider using adaptive embedding strategies based on carrier characteristics"
                        .to_string(),
                expected_improvement: 0.15,
                difficulty: DifficultyLevel::Medium,
                priority: PriorityLevel::High,
            },
            PerformanceRecommendation {
                category: RecommendationCategory::HardwareOptimization,
                description: "Enable GPU acceleration for large payloads".to_string(),
                expected_improvement: 0.3,
                difficulty: DifficultyLevel::Easy,
                priority: PriorityLevel::Medium,
            },
        ]
    }

    /// Calculate descriptive statistics
    fn calculate_descriptive_stats(&self, data: &[PerformanceMetrics]) -> DescriptiveStats {
        let mut desc = DescriptiveStats {
            count: data.len(),
            means: HashMap::new(),
            std_devs: HashMap::new(),
            mins: HashMap::new(),
            maxs: HashMap::new(),
            medians: HashMap::new(),
            quantiles: HashMap::new(),
            skewness: HashMap::new(),
            kurtosis: HashMap::new(),
        };

        if data.is_empty() {
            return desc;
        }

        let capacity: Vec<f32> = data.iter().map(|m| m.capacity_efficiency).collect();
        let time_ms: Vec<f32> = data.iter().map(|m| m.embedding_time_ms as f32).collect();
        let quality: Vec<f32> = data.iter().map(|m| m.quality_preservation).collect();
        let accuracy: Vec<f32> = data.iter().map(|m| m.extraction_accuracy).collect();

        record_metric("capacity_efficiency", &capacity, &mut desc);
        record_metric("embedding_time_ms", &time_ms, &mut desc);
        record_metric("quality_preservation", &quality, &mut desc);
        record_metric("extraction_accuracy", &accuracy, &mut desc);

        desc
    }
}

impl Default for DistributionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl DistributionAnalyzer {
    pub fn new() -> Self {
        Self {
            _distribution_tests: vec![
                DistributionTest::Normality,
                DistributionTest::Exponential,
                DistributionTest::Uniform,
            ],
        }
    }
}

impl StatisticalAnalyzer for DistributionAnalyzer {
    fn analyze(&self, data: &[PerformanceMetrics]) -> Result<StatisticalSummary> {
        let values: Vec<f32> = data.iter().map(|m| m.capacity_efficiency).collect();
        let n = values.len() as f32;

        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mean = values.iter().sum::<f32>() / n;
        let std = (values.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / n).sqrt();
        let min = sorted.first().copied().unwrap_or(0.0);
        let max = sorted.last().copied().unwrap_or(1.0);

        // KS statistic against three candidate distributions (lower = better fit)
        let ks_normal = ks_vs_normal(&sorted, mean, std);
        let ks_exp = if mean > 0.0 {
            ks_vs_exponential(&sorted, mean)
        } else {
            1.0
        };
        let ks_uniform = ks_vs_uniform(&sorted, min, max);

        let best_fit = if ks_normal <= ks_exp && ks_normal <= ks_uniform {
            "normal"
        } else if ks_exp <= ks_uniform {
            "exponential"
        } else {
            "uniform"
        };

        // KS p-value approximation: p ≈ 2·exp(-2·n·D²)
        let ks_pvalue = |d: f32| (2.0 * (-2.0 * n * d * d).exp()).clamp(0.0, 1.0);

        let mut goodness_of_fit = HashMap::new();
        goodness_of_fit.insert("normal".to_string(), 1.0 - ks_normal);
        goodness_of_fit.insert("exponential".to_string(), 1.0 - ks_exp);
        goodness_of_fit.insert("uniform".to_string(), 1.0 - ks_uniform);

        let mut test_p_values = HashMap::new();
        test_p_values.insert("normal".to_string(), ks_pvalue(ks_normal));
        test_p_values.insert("exponential".to_string(), ks_pvalue(ks_exp));
        test_p_values.insert("uniform".to_string(), ks_pvalue(ks_uniform));

        let mut parameters = HashMap::new();
        parameters.insert("mean".to_string(), mean);
        parameters.insert("std".to_string(), std);
        parameters.insert("min".to_string(), min);
        parameters.insert("max".to_string(), max);

        let alpha = 0.05f32;
        let test_results = vec![StatisticalTest {
            name: "ks_normality".to_string(),
            statistic: ks_normal,
            p_value: ks_pvalue(ks_normal),
            critical_value: 1.36 / n.sqrt(), // Kolmogorov critical value at α=0.05
            conclusion: if ks_pvalue(ks_normal) > alpha {
                "data is consistent with normal distribution".to_string()
            } else {
                "data deviates significantly from normal distribution".to_string()
            },
        }];

        Ok(StatisticalSummary {
            descriptive: DescriptiveStats {
                count: data.len(),
                means: HashMap::new(),
                std_devs: HashMap::new(),
                mins: HashMap::new(),
                maxs: HashMap::new(),
                medians: HashMap::new(),
                quantiles: HashMap::new(),
                skewness: HashMap::new(),
                kurtosis: HashMap::new(),
            },
            distribution: DistributionAnalysis {
                best_fit: best_fit.to_string(),
                parameters,
                goodness_of_fit,
                test_p_values,
            },
            correlations: HashMap::new(),
            outliers: OutlierAnalysis {
                outlier_indices: Vec::new(),
                outlier_scores: Vec::new(),
                threshold: 0.0,
                method: self.name().to_string(),
            },
            confidence_intervals: HashMap::new(),
            test_results,
        })
    }

    fn name(&self) -> &str {
        "DistributionAnalyzer"
    }

    fn min_sample_size(&self) -> usize {
        30
    }
}

impl CorrelationAnalyzer {
    pub fn new(method: CorrelationMethod) -> Self {
        Self { method }
    }

    /// Pearson correlation coefficient between two equal-length slices.
    fn pearson(xs: &[f32], ys: &[f32]) -> f32 {
        let n = xs.len() as f32;
        if n < 2.0 { return 0.0; }
        let xm = xs.iter().sum::<f32>() / n;
        let ym = ys.iter().sum::<f32>() / n;
        let cov: f32 = xs.iter().zip(ys).map(|(&x, &y)| (x - xm) * (y - ym)).sum();
        let sx = xs.iter().map(|&x| (x - xm).powi(2)).sum::<f32>().sqrt();
        let sy = ys.iter().map(|&y| (y - ym).powi(2)).sum::<f32>().sqrt();
        if sx == 0.0 || sy == 0.0 { 0.0 } else { (cov / (sx * sy)).clamp(-1.0, 1.0) }
    }

    /// Spearman rank correlation: Pearson applied to the ranks of each variable.
    fn spearman(xs: &[f32], ys: &[f32]) -> f32 {
        let rank = |vals: &[f32]| -> Vec<f32> {
            let mut indexed: Vec<(usize, f32)> =
                vals.iter().copied().enumerate().collect();
            indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let mut ranks = vec![0.0f32; vals.len()];
            let mut i = 0;
            while i < indexed.len() {
                let mut j = i + 1;
                while j < indexed.len() && indexed[j].1 == indexed[i].1 { j += 1; }
                let avg_rank = (i + j - 1) as f32 / 2.0;
                for k in i..j { ranks[indexed[k].0] = avg_rank; }
                i = j;
            }
            ranks
        };
        Self::pearson(&rank(xs), &rank(ys))
    }

    /// Dispatch to the configured correlation method.
    fn correlate(&self, xs: &[f32], ys: &[f32]) -> f32 {
        match self.method {
            CorrelationMethod::Pearson => Self::pearson(xs, ys),
            CorrelationMethod::Spearman | CorrelationMethod::Kendall => Self::spearman(xs, ys),
        }
    }
}

impl StatisticalAnalyzer for CorrelationAnalyzer {
    fn analyze(&self, data: &[PerformanceMetrics]) -> Result<StatisticalSummary> {
        let mut correlations = HashMap::new();

        if data.len() >= 2 {
            let capacity: Vec<f32> = data.iter().map(|m| m.capacity_efficiency).collect();
            let time_ms:  Vec<f32> = data.iter().map(|m| m.embedding_time_ms as f32).collect();
            let quality:  Vec<f32> = data.iter().map(|m| m.quality_preservation).collect();
            let accuracy: Vec<f32> = data.iter().map(|m| m.extraction_accuracy).collect();

            correlations.insert(
                "capacity_time_correlation".to_string(),
                self.correlate(&capacity, &time_ms),
            );
            correlations.insert(
                "capacity_quality_correlation".to_string(),
                self.correlate(&capacity, &quality),
            );
            correlations.insert(
                "time_quality_correlation".to_string(),
                self.correlate(&time_ms, &quality),
            );
            correlations.insert(
                "capacity_accuracy_correlation".to_string(),
                self.correlate(&capacity, &accuracy),
            );
        }

        Ok(StatisticalSummary {
            descriptive: DescriptiveStats {
                count: data.len(),
                means: HashMap::new(),
                std_devs: HashMap::new(),
                mins: HashMap::new(),
                maxs: HashMap::new(),
                medians: HashMap::new(),
                quantiles: HashMap::new(),
                skewness: HashMap::new(),
                kurtosis: HashMap::new(),
            },
            distribution: DistributionAnalysis {
                best_fit: "unknown".to_string(),
                parameters: HashMap::new(),
                goodness_of_fit: HashMap::new(),
                test_p_values: HashMap::new(),
            },
            correlations,
            outliers: OutlierAnalysis {
                outlier_indices: Vec::new(),
                outlier_scores: Vec::new(),
                threshold: 0.0,
                method: self.name().to_string(),
            },
            confidence_intervals: HashMap::new(),
            test_results: Vec::new(),
        })
    }

    fn name(&self) -> &str {
        "CorrelationAnalyzer"
    }

    fn min_sample_size(&self) -> usize {
        10
    }
}

impl OutlierAnalyzer {
    pub fn new(method: OutlierMethod, threshold: f32) -> Self {
        Self { method, threshold }
    }

    /// IQR fence method: outliers are values beyond Q1 − k·IQR or Q3 + k·IQR.
    fn iqr_outliers(&self, values: &[f32]) -> (Vec<usize>, Vec<f32>) {
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q1 = percentile(&sorted, 0.25);
        let q3 = percentile(&sorted, 0.75);
        let iqr = q3 - q1;
        if iqr == 0.0 {
            return (vec![], vec![]);
        }
        let lower = q1 - self.threshold * iqr;
        let upper = q3 + self.threshold * iqr;

        values
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                if v < lower || v > upper {
                    let score = if v < lower {
                        (lower - v) / iqr
                    } else {
                        (v - upper) / iqr
                    };
                    Some((i, score))
                } else {
                    None
                }
            })
            .unzip()
    }

    /// Z-score method: outliers have |z| > threshold.
    fn zscore_outliers(&self, values: &[f32]) -> (Vec<usize>, Vec<f32>) {
        let n = values.len() as f32;
        if n < 2.0 {
            return (vec![], vec![]);
        }
        let mean = values.iter().sum::<f32>() / n;
        let std = (values.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / n).sqrt();
        if std == 0.0 {
            return (vec![], vec![]);
        }
        values
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                let z = (v - mean).abs() / std;
                if z > self.threshold { Some((i, z)) } else { None }
            })
            .unzip()
    }

    /// Modified Z-score method: uses median and MAD — more robust to extreme outliers.
    fn modified_zscore_outliers(&self, values: &[f32]) -> (Vec<usize>, Vec<f32>) {
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = percentile(&sorted, 0.5);

        let mut abs_devs: Vec<f32> = values.iter().map(|&x| (x - median).abs()).collect();
        abs_devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = percentile(&abs_devs, 0.5);
        let scale = if mad > 0.0 { mad * 1.4826 } else { 1.0 }; // ≈ σ for normal data

        values
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                let mz = (v - median).abs() / scale;
                if mz > self.threshold { Some((i, mz)) } else { None }
            })
            .unzip()
    }
}

impl StatisticalAnalyzer for OutlierAnalyzer {
    fn analyze(&self, data: &[PerformanceMetrics]) -> Result<StatisticalSummary> {
        let values: Vec<f32> = data.iter().map(|m| m.capacity_efficiency).collect();

        let (outlier_indices, outlier_scores) = match self.method {
            OutlierMethod::IQR => self.iqr_outliers(&values),
            OutlierMethod::ZScore => self.zscore_outliers(&values),
            OutlierMethod::ModifiedZScore => self.modified_zscore_outliers(&values),
            OutlierMethod::IsolationForest => self.zscore_outliers(&values), // CPU fallback
        };

        let outliers = OutlierAnalysis {
            outlier_indices,
            outlier_scores,
            threshold: self.threshold,
            method: format!("{:?}", self.method),
        };

        Ok(StatisticalSummary {
            descriptive: DescriptiveStats {
                count: data.len(),
                means: HashMap::new(),
                std_devs: HashMap::new(),
                mins: HashMap::new(),
                maxs: HashMap::new(),
                medians: HashMap::new(),
                quantiles: HashMap::new(),
                skewness: HashMap::new(),
                kurtosis: HashMap::new(),
            },
            distribution: DistributionAnalysis {
                best_fit: "unknown".to_string(),
                parameters: HashMap::new(),
                goodness_of_fit: HashMap::new(),
                test_p_values: HashMap::new(),
            },
            correlations: HashMap::new(),
            outliers,
            confidence_intervals: HashMap::new(),
            test_results: Vec::new(),
        })
    }

    fn name(&self) -> &str {
        "OutlierAnalyzer"
    }

    fn min_sample_size(&self) -> usize {
        20
    }
}

impl ChiSquareDetector {
    pub fn new(alpha: f32) -> Self {
        Self { _alpha: alpha }
    }
}

impl DetectionEvaluator for ChiSquareDetector {
    /// Real chi-square test comparing the byte-frequency distributions of `original`
    /// and `stego`.  Uses the Wilson-Hilferty normal approximation to convert the
    /// χ²(255) statistic to a p-value without requiring an incomplete-gamma library.
    fn evaluate(&self, original: &[u8], stego: &[u8]) -> Result<DetectionScore> {
        // Byte frequency histograms
        let mut orig_hist = [0u64; 256];
        let mut stego_hist = [0u64; 256];
        for &b in original { orig_hist[b as usize] += 1; }
        for &b in stego   { stego_hist[b as usize] += 1; }

        let orig_n  = original.len() as f64;
        let stego_n = stego.len()    as f64;

        // χ² = Σ (observed − expected)² / expected
        // expected[i] = orig_hist[i] / orig_n * stego_n   (scale original distribution to stego size)
        let df = 255.0_f64;
        let mut chi2 = 0.0_f64;
        let mut active_bins = 0usize;

        for i in 0..256 {
            if orig_n > 0.0 {
                let expected = orig_hist[i] as f64 / orig_n * stego_n;
                if expected >= 0.5 {       // skip near-zero expected cells
                    let diff = stego_hist[i] as f64 - expected;
                    chi2 += diff * diff / expected;
                    active_bins += 1;
                }
            }
        }

        // Wilson-Hilferty approximation: χ²(df) → standard normal
        // z ≈ ((χ²/df)^(1/3) − (1 − 2/(9·df))) / √(2/(9·df))
        let effective_df = active_bins.saturating_sub(1).max(1) as f64;
        let x   = (chi2 / effective_df).cbrt();
        let mu  = 1.0 - 2.0 / (9.0 * effective_df);
        let sig = (2.0 / (9.0 * effective_df)).sqrt();
        let z   = ((x - mu) / sig) as f32;

        // p-value = P(χ²(df) > chi2) = 1 − Φ(z)
        let p_value = 1.0 - normal_cdf(z);

        // Low p-value → distributions differ → steganography is detectable
        let detection_probability = (1.0 - p_value).clamp(0.0, 1.0);

        let confidence = if orig_n >= 256.0 && stego_n >= 256.0 { 0.85 } else { 0.5 };

        let resistance_grade = match detection_probability {
            p if p < 0.2 => ResistanceGrade::Excellent,
            p if p < 0.4 => ResistanceGrade::Good,
            p if p < 0.6 => ResistanceGrade::Fair,
            p if p < 0.8 => ResistanceGrade::Poor,
            _            => ResistanceGrade::Failed,
        };

        let mut method_scores = HashMap::new();
        method_scores.insert("chi2_statistic".to_string(), chi2 as f32);
        method_scores.insert("p_value".to_string(), p_value);
        method_scores.insert("degrees_of_freedom".to_string(), df as f32);

        Ok(DetectionScore {
            detection_probability,
            confidence,
            method_scores,
            evidence: vec![DetectionEvidence {
                description: format!(
                    "χ²={:.2} ({} bins, df={}), p={:.4}",
                    chi2, active_bins, effective_df as usize, p_value
                ),
                strength: detection_probability,
                method: self.name().to_string(),
                location: None,
            }],
            resistance_grade,
        })
    }

    fn name(&self) -> &str {
        "ChiSquareDetector"
    }

    fn category(&self) -> DetectionCategory {
        DetectionCategory::Statistical
    }
}

impl DetectionEvaluator for SamplePairsDetector {
    /// Sample Pairs (SP) analysis: compares adjacent-pair difference histograms.
    ///
    /// After LSB substitution, the D_0 (same-value) and D_1 (differ-by-1) counts
    /// shift in a characteristic way.  The SP embedding estimate is derived from
    /// the change in the D_0 / D_1 ratio between the original and stego data.
    fn evaluate(&self, original: &[u8], stego: &[u8]) -> Result<DetectionScore> {
        let n = original.len().min(stego.len());

        if n < 2 {
            return Ok(DetectionScore {
                detection_probability: 0.0,
                confidence: 0.0,
                method_scores: HashMap::new(),
                evidence: Vec::new(),
                resistance_grade: ResistanceGrade::Excellent,
            });
        }

        // Count D_0 (|diff|=0) and D_1 (|diff|=1) for both original and stego
        let (mut o_d0, mut o_d1) = (0u64, 0u64);
        let (mut s_d0, mut s_d1) = (0u64, 0u64);
        for i in 0..n - 1 {
            let o_diff = (original[i] as i16 - original[i + 1] as i16).unsigned_abs();
            if o_diff == 0 { o_d0 += 1; } else if o_diff == 1 { o_d1 += 1; }

            let s_diff = (stego[i] as i16 - stego[i + 1] as i16).unsigned_abs();
            if s_diff == 0 { s_d0 += 1; } else if s_diff == 1 { s_d1 += 1; }
        }

        let pairs = (n - 1) as f32;
        let o_d0r = o_d0 as f32 / pairs;
        let o_d1r = o_d1 as f32 / pairs;
        let s_d0r = s_d0 as f32 / pairs;
        let s_d1r = s_d1 as f32 / pairs;

        // SP ratio: (D_0 - D_1) / (D_0 + D_1); LSB embedding drives this toward 0
        let sp_orig = if o_d0 + o_d1 > 0 {
            (o_d0 as f32 - o_d1 as f32) / (o_d0 + o_d1) as f32
        } else {
            0.0
        };
        let sp_stego = if s_d0 + s_d1 > 0 {
            (s_d0 as f32 - s_d1 as f32) / (s_d0 + s_d1) as f32
        } else {
            0.0
        };
        let sp_change = (sp_stego - sp_orig).abs();
        let d0_shift = (s_d0r - o_d0r).abs();
        let d1_shift = (s_d1r - o_d1r).abs();

        // Weight SP change more; scale so typical embedding (~25% payload) gives ~0.4
        let detection_probability = (sp_change * 2.0 + d0_shift + d1_shift).clamp(0.0, 1.0);
        let confidence = if n >= 1000 { 0.80 } else { 0.50 };

        let resistance_grade = match detection_probability {
            p if p < 0.2 => ResistanceGrade::Excellent,
            p if p < 0.4 => ResistanceGrade::Good,
            p if p < 0.6 => ResistanceGrade::Fair,
            p if p < 0.8 => ResistanceGrade::Poor,
            _ => ResistanceGrade::Failed,
        };

        let mut method_scores = HashMap::new();
        method_scores.insert("sp_ratio_change".to_string(), sp_change);
        method_scores.insert("d0_rate_shift".to_string(), d0_shift);
        method_scores.insert("d1_rate_shift".to_string(), d1_shift);

        Ok(DetectionScore {
            detection_probability,
            confidence,
            method_scores,
            evidence: vec![DetectionEvidence {
                description: format!(
                    "SP analysis: D₀ shift={:.4}, D₁ shift={:.4}, SP ratio change={:.4}",
                    d0_shift, d1_shift, sp_change
                ),
                strength: detection_probability,
                method: self.name().to_string(),
                location: None,
            }],
            resistance_grade,
        })
    }

    fn name(&self) -> &str {
        "SamplePairsDetector"
    }

    fn category(&self) -> DetectionCategory {
        DetectionCategory::Statistical
    }
}

impl DetectionEvaluator for HistogramDetector {
    /// Compare byte-value histograms via Total Variation Distance (TVD = L₁/2).
    ///
    /// TVD = ½ Σ |p_i − q_i|.  For identical distributions TVD=0; for completely
    /// disjoint distributions TVD=1.  Even small LSB changes produce measurable TVD.
    fn evaluate(&self, original: &[u8], stego: &[u8]) -> Result<DetectionScore> {
        let mut orig_hist = [0u64; 256];
        let mut stego_hist = [0u64; 256];
        for &b in original { orig_hist[b as usize] += 1; }
        for &b in stego { stego_hist[b as usize] += 1; }

        let orig_n = original.len().max(1) as f64;
        let stego_n = stego.len().max(1) as f64;

        // Total Variation Distance
        let tvd: f64 = orig_hist
            .iter()
            .zip(stego_hist.iter())
            .map(|(&o, &s)| ((o as f64 / orig_n) - (s as f64 / stego_n)).abs())
            .sum::<f64>()
            / 2.0;

        // Peak single-bin difference (most-changed byte value)
        let peak_diff: f64 = orig_hist
            .iter()
            .zip(stego_hist.iter())
            .map(|(&o, &s)| ((o as f64 / orig_n) - (s as f64 / stego_n)).abs())
            .fold(0.0_f64, f64::max);

        // Chi-square between the two histograms (for bins with expected > 0)
        let chi2: f64 = orig_hist
            .iter()
            .zip(stego_hist.iter())
            .filter_map(|(&o, &s)| {
                if o > 0 {
                    let e = o as f64 / orig_n * stego_n;
                    Some((s as f64 - e).powi(2) / e)
                } else {
                    None
                }
            })
            .sum();

        // Scale TVD from [0, 0.5] to [0, 1]; typical steganography produces TVD ~0.01-0.1
        let detection_probability = (tvd * 4.0).clamp(0.0, 1.0) as f32;
        let confidence = if original.len() >= 256 && stego.len() >= 256 { 0.75 } else { 0.40 };

        let resistance_grade = match detection_probability {
            p if p < 0.2 => ResistanceGrade::Excellent,
            p if p < 0.4 => ResistanceGrade::Good,
            p if p < 0.6 => ResistanceGrade::Fair,
            p if p < 0.8 => ResistanceGrade::Poor,
            _ => ResistanceGrade::Failed,
        };

        let mut method_scores = HashMap::new();
        method_scores.insert("total_variation_distance".to_string(), tvd as f32);
        method_scores.insert("peak_bin_difference".to_string(), peak_diff as f32);
        method_scores.insert("chi2_statistic".to_string(), chi2 as f32);

        Ok(DetectionScore {
            detection_probability,
            confidence,
            method_scores,
            evidence: vec![DetectionEvidence {
                description: format!(
                    "Histogram TVD={:.4}, peak bin diff={:.4}, χ²={:.2}",
                    tvd, peak_diff, chi2
                ),
                strength: detection_probability,
                method: self.name().to_string(),
                location: None,
            }],
            resistance_grade,
        })
    }

    fn name(&self) -> &str {
        "HistogramDetector"
    }

    fn category(&self) -> DetectionCategory {
        DetectionCategory::Visual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_config_default() {
        let config = AnalysisConfig::default();
        assert!(config.statistical_analysis);
        assert!(config.detection_testing);
        assert_eq!(config.confidence_level, 0.95);
    }

    #[test]
    fn test_distribution_analyzer() {
        let analyzer = DistributionAnalyzer::new();
        assert_eq!(analyzer.name(), "DistributionAnalyzer");
        assert_eq!(analyzer.min_sample_size(), 30);
    }

    #[test]
    fn test_chi_square_detector() {
        let detector = ChiSquareDetector::new(0.05);
        assert_eq!(detector.name(), "ChiSquareDetector");
        assert!(matches!(
            detector.category(),
            DetectionCategory::Statistical
        ));
    }

    #[test]
    fn test_resistance_grade_ordering() {
        // Test that resistance grades can be compared
        let excellent = ResistanceGrade::Excellent;
        let poor = ResistanceGrade::Poor;

        // These should be different variants
        assert!(matches!(excellent, ResistanceGrade::Excellent));
        assert!(matches!(poor, ResistanceGrade::Poor));
    }

    #[tokio::test]
    async fn test_analysis_engine() {
        let config = AnalysisConfig::default();
        let engine = AnalysisEngine::new(config);

        assert!(!engine.analyzers.is_empty());
        assert!(!engine.detection_evaluators.is_empty());
    }

    fn make_metrics(capacity: f32, time_ms: u64, quality: f32, accuracy: f32) -> PerformanceMetrics {
        PerformanceMetrics {
            capacity_efficiency: capacity,
            embedding_time_ms: time_ms,
            quality_preservation: quality,
            extraction_accuracy: accuracy,
            memory_usage: 1024,
            gpu_acceleration_benefit: 1.0,
        }
    }

    #[test]
    fn test_descriptive_stats_complete() {
        let engine = AnalysisEngine::new(AnalysisConfig::default());
        let data: Vec<PerformanceMetrics> = (0..10)
            .map(|i| make_metrics(i as f32 / 10.0, i as u64 * 10, 0.9, 0.95))
            .collect();

        let desc = engine.calculate_descriptive_stats(&data);
        assert_eq!(desc.count, 10);
        // All fields should be populated
        assert!(!desc.std_devs.is_empty(), "std_devs should be populated");
        assert!(!desc.medians.is_empty(), "medians should be populated");
        assert!(!desc.quantiles.is_empty(), "quantiles should be populated");
        assert!(!desc.skewness.is_empty(), "skewness should be populated");
        assert!(!desc.kurtosis.is_empty(), "kurtosis should be populated");
        // Sanity-check mean of capacity_efficiency = 0 + 0.1 + ... + 0.9 / 10 = 0.45
        let mean = desc.means["capacity_efficiency"];
        assert!((mean - 0.45).abs() < 0.01, "mean capacity: expected ≈0.45, got {}", mean);
        // Q1 < median < Q3
        let (q1, q3) = desc.quantiles["capacity_efficiency"];
        let med = desc.medians["capacity_efficiency"];
        assert!(q1 <= med && med <= q3, "Q1={} median={} Q3={}", q1, med, q3);
    }

    #[test]
    fn test_iqr_outlier_detection() {
        let analyzer = OutlierAnalyzer::new(OutlierMethod::IQR, 1.5);
        // Values with one clear outlier
        let metrics: Vec<PerformanceMetrics> = (0..20)
            .map(|i| {
                let cap = if i == 19 { 10.0 } else { i as f32 / 20.0 }; // 10.0 is far outlier
                make_metrics(cap, 10, 0.9, 0.95)
            })
            .collect();

        let summary = analyzer.analyze(&metrics).unwrap();
        assert!(
            !summary.outliers.outlier_indices.is_empty(),
            "IQR should detect the extreme outlier at index 19"
        );
        assert!(summary.outliers.outlier_indices.contains(&19));
        assert_eq!(summary.outliers.method, "IQR");
    }

    #[test]
    fn test_zscore_outlier_detection() {
        let analyzer = OutlierAnalyzer::new(OutlierMethod::ZScore, 2.0);
        let metrics: Vec<PerformanceMetrics> = (0..20)
            .map(|i| {
                let cap = if i == 0 { 50.0 } else { 0.5 + i as f32 * 0.01 };
                make_metrics(cap, 10, 0.9, 0.95)
            })
            .collect();

        let summary = analyzer.analyze(&metrics).unwrap();
        assert!(
            !summary.outliers.outlier_indices.is_empty(),
            "Z-score should detect the outlier at index 0"
        );
        assert!(summary.outliers.outlier_indices.contains(&0));
    }

    #[test]
    fn test_distribution_analyzer_identifies_best_fit() {
        let analyzer = DistributionAnalyzer::new();
        // Uniformly distributed capacity values → uniform should win or at least be considered
        let metrics: Vec<PerformanceMetrics> = (0..50)
            .map(|i| make_metrics(i as f32 / 50.0, 10, 0.9, 0.95))
            .collect();

        let summary = analyzer.analyze(&metrics).unwrap();
        // best_fit should be one of the known distributions
        assert!(
            ["normal", "exponential", "uniform"].contains(&summary.distribution.best_fit.as_str()),
            "unexpected best_fit: {}",
            summary.distribution.best_fit
        );
        // All three should have goodness-of-fit scores
        assert!(summary.distribution.goodness_of_fit.contains_key("normal"));
        assert!(summary.distribution.goodness_of_fit.contains_key("uniform"));
        // p-values should be in [0, 1]
        for (_, &p) in &summary.distribution.test_p_values {
            assert!(p >= 0.0 && p <= 1.0, "p-value out of range: {}", p);
        }
    }

    #[test]
    fn test_sample_pairs_detector_identical_data() {
        let detector = SamplePairsDetector;
        let data: Vec<u8> = (0..1000u16).map(|i| (i % 256) as u8).collect();
        // Identical original and stego → near-zero detection probability
        let score = detector.evaluate(&data, &data).unwrap();
        assert!(
            score.detection_probability < 0.05,
            "identical data should give near-zero detection, got {}",
            score.detection_probability
        );
    }

    #[test]
    fn test_sample_pairs_detector_lsb_flipped() {
        let detector = SamplePairsDetector;
        let original: Vec<u8> = (0..2000u16).map(|i| (i % 256) as u8).collect();
        // Simulate aggressive LSB embedding: flip all LSBs
        let stego: Vec<u8> = original.iter().map(|&b| b ^ 1).collect();
        let score = detector.evaluate(&original, &stego).unwrap();
        // Flipping all LSBs should significantly change the D0/D1 histogram
        assert!(
            score.detection_probability > 0.0,
            "all-LSB-flipped data should produce non-zero detection"
        );
        assert!(!score.method_scores.is_empty());
    }

    #[test]
    fn test_histogram_detector_identical() {
        let detector = HistogramDetector;
        let data: Vec<u8> = (0..512u16).map(|i| (i % 256) as u8).collect();
        let score = detector.evaluate(&data, &data).unwrap();
        assert!(
            score.detection_probability < 0.01,
            "identical data → TVD=0 → detection ≈ 0, got {}",
            score.detection_probability
        );
        let tvd = score.method_scores["total_variation_distance"];
        assert!(tvd < 0.001, "TVD should be 0 for identical data, got {}", tvd);
    }

    #[test]
    fn test_histogram_detector_different_distributions() {
        let detector = HistogramDetector;
        let original: Vec<u8> = vec![0u8; 512];
        let stego: Vec<u8> = (0..512u16).map(|i| (i % 256) as u8).collect();
        let score = detector.evaluate(&original, &stego).unwrap();
        // Very different distributions should produce high detection probability
        assert!(
            score.detection_probability > 0.5,
            "very different histograms should be detected, got {}",
            score.detection_probability
        );
        assert!(score.method_scores.contains_key("total_variation_distance"));
        assert!(score.method_scores.contains_key("chi2_statistic"));
    }
}
