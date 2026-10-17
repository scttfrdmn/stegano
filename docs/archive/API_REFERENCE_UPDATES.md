# API Reference Updates

This document covers the new APIs introduced for advanced ML optimization and scientific format support in Stegano v0.2.0.

## Table of Contents

1. [ML Optimization APIs](#ml-optimization-apis)
2. [Scientific Format Handler APIs](#scientific-format-handler-apis)
3. [Objective Functions](#objective-functions)
4. [Constraint APIs](#constraint-apis)
5. [Configuration Types](#configuration-types)
6. [Error Types](#error-types)
7. [Usage Examples](#usage-examples)

## ML Optimization APIs

### OptimizationEngine

The main interface for ML-based parameter optimization.

```rust
pub struct OptimizationEngine {
    pub config: OptimizationConfig,
    objectives: Vec<Box<dyn ObjectiveFunction>>,
    constraints: Vec<Box<dyn OptimizationConstraint>>,
}
```

#### Methods

##### `new(config: OptimizationConfig) -> Self`

Creates a new optimization engine with the specified configuration.

```rust
let config = OptimizationConfig {
    algorithm: OptimizationAlgorithm::GeneticAlgorithm {
        mutation_rate: 0.1,
        crossover_rate: 0.8,
    },
    max_iterations: 100,
    population_size: 50,
    tolerance: 0.001,
    parallel_config: ParallelOptimizationConfig::default(),
    multi_objective: MultiObjectiveConfig::default(),
};

let engine = OptimizationEngine::new(config);
```

##### `add_objective(&mut self, objective: Box<dyn ObjectiveFunction>)`

Adds an objective function to optimize.

```rust
engine.add_objective(Box::new(StealthObjective { weight: 1.0 }));
engine.add_objective(Box::new(QualityObjective { weight: 0.8 }));
```

##### `add_constraint(&mut self, constraint: Box<dyn OptimizationConstraint>)`

Adds a constraint that solutions must satisfy.

```rust
engine.add_constraint(Box::new(MemoryConstraint { max_memory_mb: 512 }));
```

##### `optimize(&mut self, carrier: &CarrierData, payload: &[u8], initial_params: Option<EmbedParams>) -> Result<OptimizationResult>`

Performs single-objective optimization.

```rust
let result = engine.optimize(&carrier, payload, None).await?;
println!("Best score: {}", result.best_objective_value.combined_score);
```

##### `multi_objective_optimization(&mut self, carrier: &CarrierData, payload: &[u8]) -> Result<OptimizationResult>`

Performs multi-objective optimization using NSGA-II.

```rust
let result = engine.multi_objective_optimization(&carrier, payload).await?;
println!("Found {} Pareto-optimal solutions", result.pareto_front.len());
```

### OptimizationAlgorithm

Enumeration of available optimization algorithms.

```rust
pub enum OptimizationAlgorithm {
    GeneticAlgorithm {
        mutation_rate: f32,
        crossover_rate: f32,
    },
    ParticleSwarm {
        inertia: f32,
        cognitive: f32,
        social: f32,
    },
    BayesianOptimization {
        acquisition_function: AcquisitionFunction,
    },
}
```

### AcquisitionFunction

Acquisition functions for Bayesian optimization.

```rust
pub enum AcquisitionFunction {
    ExpectedImprovement,
    UpperConfidenceBound { kappa: f64 },
    ProbabilityOfImprovement,
}
```

### OptimizationResult

Result structure containing optimization outcomes.

```rust
pub struct OptimizationResult {
    pub best_parameters: EmbedParams,
    pub best_objective_value: ObjectiveValue,
    pub pareto_front: Vec<(EmbedParams, ObjectiveValue)>,
    pub convergence: ConvergenceInfo,
    pub optimization_time: Duration,
}
```

### Bayesian Optimization APIs

#### GaussianProcess

Gaussian process surrogate model for Bayesian optimization.

```rust
pub struct GaussianProcess {
    kernel: Box<dyn Kernel>,
    noise_variance: f64,
}

impl GaussianProcess {
    pub fn new(kernel: Box<dyn Kernel>) -> Self
    pub fn fit(&mut self, x: &Array2<f64>, y: &Array1<f64>) -> Result<()>
    pub fn predict(&self, x: &Array2<f64>) -> Result<(Array1<f64>, Array1<f64>)>
    pub fn log_marginal_likelihood(&self) -> f64
}
```

#### Kernel Implementations

```rust
pub trait Kernel: Send + Sync {
    fn compute(&self, x1: &Array2<f64>, x2: &Array2<f64>) -> Array2<f64>;
    fn compute_diagonal(&self, x: &Array2<f64>) -> Array1<f64>;
}

pub struct RBFKernel {
    pub length_scale: f64,
    pub variance: f64,
}

pub struct MaternKernel {
    pub length_scale: f64,
    pub variance: f64,
    pub nu: f64,
}

pub struct RationalQuadraticKernel {
    pub length_scale: f64,
    pub variance: f64,
    pub alpha: f64,
}
```

### Reinforcement Learning APIs

#### RLAgent

Q-learning agent for adaptive strategy selection.

```rust
pub struct RLAgent {
    config: RLConfig,
    q_table: HashMap<(StateRepresentation, OptimizationAlgorithmType), f64>,
    policy_network: Option<PolicyNetwork>,
    experience_buffer: ExperienceReplayBuffer,
}

impl RLAgent {
    pub fn new(config: RLConfig) -> Self
    pub async fn train(&mut self, episodes: &[Episode]) -> Result<TrainingResult>
    pub fn select_strategy(&self, state: &StateRepresentation) -> Result<OptimizationAlgorithmType>
    pub fn update_q_value(&mut self, state: StateRepresentation, action: OptimizationAlgorithmType, reward: f64, next_state: StateRepresentation)
}
```

#### ExplorationStrategy

Exploration strategies for reinforcement learning.

```rust
pub enum ExplorationStrategy {
    EpsilonGreedy,
    UCB { c: f64 },
    ThompsonSampling,
    Softmax { temperature: f64 },
}
```

## Scientific Format Handler APIs

### VcfHandler

Handler for Variant Call Format (genomics) files.

```rust
pub struct VcfHandler {
    strategy: VcfEmbeddingStrategy,
}

impl VcfHandler {
    pub fn new() -> Self
    
    pub async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams) -> Result<EmbedResult>
    
    pub async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>
    
    pub fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>
    
    pub fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>
}
```

#### VcfEmbeddingStrategy

Embedding strategies for VCF files.

```rust
pub enum VcfEmbeddingStrategy {
    InfoFields {
        preserve_critical_annotations: bool,
        target_info_fields: Vec<String>,
    },
    QualityPrecision {
        max_precision_loss: f64,
        min_quality_threshold: f64,
        preserve_low_quality_variants: bool,
    },
    FormatFields {
        target_samples: Vec<String>,
        preserve_genotypes: bool,
        target_format_fields: Vec<String>,
    },
}
```

### DicomHandler

Handler for Digital Imaging and Communications in Medicine files.

```rust
pub struct DicomHandler {
    strategy: DicomEmbeddingStrategy,
}

impl DicomHandler {
    pub fn new() -> Self
    
    pub async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams) -> Result<EmbedResult>
    
    pub async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>
    
    pub fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>
    
    pub fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>
    
    pub fn verify_dicom_compliance(&self, carrier: &CarrierData) -> Result<ComplianceReport>
}
```

#### DicomEmbeddingStrategy

Embedding strategies for DICOM files.

```rust
pub enum DicomEmbeddingStrategy {
    PrivateElements {
        private_group_range: (u16, u16),
        preserve_clinical_data: bool,
        maintain_sop_class: bool,
    },
    PixelLSB {
        max_pixel_change: u16,
        preserve_diagnostic_regions: bool,
        bits_per_pixel: u8,
        exclude_roi_masks: Vec<String>,
    },
    UnusedMetadata {
        preserve_required_tags: bool,
        max_field_length: usize,
        exclude_patient_tags: bool,
        target_unused_tags: Vec<String>,
    },
}
```

### SamBamHandler

Handler for Sequence Alignment/Map and Binary Alignment/Map files.

```rust
pub struct SamBamHandler {
    strategy: SamBamEmbeddingStrategy,
}

impl SamBamHandler {
    pub fn new() -> Self
    
    pub async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams) -> Result<EmbedResult>
    
    pub async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>
    
    pub fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>
    
    pub fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>
    
    pub fn verify_alignment_quality(&self, original: &CarrierData, modified: &CarrierData) -> Result<AlignmentQualityMetrics>
}
```

#### SamBamEmbeddingStrategy

Embedding strategies for SAM/BAM files.

```rust
pub enum SamBamEmbeddingStrategy {
    OptionalFields {
        preserve_core_alignment: bool,
        max_field_size: usize,
        target_optional_tags: Vec<String>,
    },
    QualityScores {
        min_quality_threshold: u8,
        max_quality_change: u8,
        preserve_low_quality: bool,
        use_sliding_window: bool,
    },
    CommentLines {
        preserve_existing_comments: bool,
        max_comment_length: usize,
        comment_prefix: String,
    },
}
```

## Objective Functions

### ObjectiveFunction Trait

Base trait for optimization objectives.

```rust
pub trait ObjectiveFunction: Send + Sync {
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, payload: &[u8]) -> Result<f32>;
    fn name(&self) -> &str;
    fn maximize(&self) -> bool;
    fn weight(&self) -> f32;
}
```

### Built-in Objectives

#### StealthObjective

Evaluates embedding stealth/detectability.

```rust
pub struct StealthObjective {
    pub weight: f32,
}

impl ObjectiveFunction for StealthObjective {
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, payload: &[u8]) -> Result<f32> {
        // Implementation evaluates stealth metrics
        // Higher values indicate better stealth
    }
}
```

#### QualityObjective

Evaluates carrier data quality preservation.

```rust
pub struct QualityObjective {
    pub weight: f32,
}

impl ObjectiveFunction for QualityObjective {
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, payload: &[u8]) -> Result<f32> {
        // Implementation evaluates quality preservation
        // Higher values indicate better quality preservation
    }
}
```

#### CapacityObjective

Evaluates embedding capacity utilization.

```rust
pub struct CapacityObjective {
    pub weight: f32,
}
```

#### Scientific-Specific Objectives

```rust
// For genomics data
pub struct GenomicsIntegrityObjective {
    pub weight: f32,
}

// For medical imaging
pub struct MedicalComplianceObjective {
    pub weight: f32,
}

pub struct DiagnosticQualityObjective {
    pub weight: f32,
}

// For bioinformatics
pub struct BioinformaticsIntegrityObjective {
    pub weight: f32,
}

pub struct AlignmentQualityObjective {
    pub weight: f32,
}
```

## Constraint APIs

### OptimizationConstraint Trait

Base trait for optimization constraints.

```rust
pub trait OptimizationConstraint: Send + Sync {
    fn is_satisfied(&self, params: &EmbedParams, carrier: &CarrierData) -> bool;
    fn name(&self) -> &str;
}
```

### Built-in Constraints

#### MemoryConstraint

Limits memory usage during embedding.

```rust
pub struct MemoryConstraint {
    pub max_memory_mb: usize,
}

impl OptimizationConstraint for MemoryConstraint {
    fn is_satisfied(&self, params: &EmbedParams, carrier: &CarrierData) -> bool {
        // Check estimated memory usage
    }
}
```

#### TimeConstraint

Limits execution time for embedding.

```rust
pub struct TimeConstraint {
    pub max_time_ms: u64,
}
```

#### Scientific-Specific Constraints

```rust
// For genomics
pub struct AlleleFrequencyConstraint {
    pub min_maf: f64, // Minimum allele frequency
}

pub struct HardyWeinbergConstraint {
    pub p_value_threshold: f64,
}

// For medical imaging
pub struct FDAComplianceConstraint;

pub struct DiagnosticIntegrityConstraint {
    pub min_quality: f64,
}

// For bioinformatics
pub struct MappingQualityConstraint {
    pub min_mapq: u8,
}

pub struct AlignmentCoverageConstraint {
    pub min_coverage: f64,
}
```

## Configuration Types

### OptimizationConfig

Main configuration structure for optimization.

```rust
pub struct OptimizationConfig {
    pub algorithm: OptimizationAlgorithm,
    pub max_iterations: usize,
    pub population_size: usize,
    pub tolerance: f32,
    pub parallel_config: ParallelOptimizationConfig,
    pub multi_objective: MultiObjectiveConfig,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            algorithm: OptimizationAlgorithm::GeneticAlgorithm {
                mutation_rate: 0.1,
                crossover_rate: 0.7,
            },
            max_iterations: 100,
            population_size: 50,
            tolerance: 0.001,
            parallel_config: ParallelOptimizationConfig::default(),
            multi_objective: MultiObjectiveConfig::default(),
        }
    }
}
```

### ParallelOptimizationConfig

Configuration for parallel optimization.

```rust
pub struct ParallelOptimizationConfig {
    pub enabled: bool,
    pub num_workers: usize,
    pub batch_size: usize,
}

impl Default for ParallelOptimizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            num_workers: num_cpus::get(),
            batch_size: 10,
        }
    }
}
```

### MultiObjectiveConfig

Configuration for multi-objective optimization.

```rust
pub struct MultiObjectiveConfig {
    pub enabled: bool,
    pub objective_weights: HashMap<String, f32>,
    pub pareto_method: ParetoMethod,
    pub max_pareto_solutions: usize,
}

pub enum ParetoMethod {
    NSGA2,
    SPEA2,
    MOGA,
}
```

### RLConfig

Configuration for reinforcement learning.

```rust
pub struct RLConfig {
    pub learning_rate: f64,
    pub discount_factor: f64,
    pub exploration_config: ExplorationConfig,
    pub max_episodes: usize,
    pub episode_length: usize,
    pub experience_buffer_size: usize,
    pub batch_size: usize,
    pub target_update_frequency: usize,
}

pub struct ExplorationConfig {
    pub initial_epsilon: f64,
    pub final_epsilon: f64,
    pub decay_rate: f64,
    pub strategy: ExplorationStrategy,
}
```

## Error Types

### OptimizationError

Errors specific to optimization algorithms.

```rust
pub enum OptimizationError {
    ConvergenceFailure {
        iterations: usize,
        best_score: f32,
    },
    InvalidConfiguration {
        message: String,
    },
    ObjectiveEvaluationFailed {
        objective_name: String,
        error: Box<dyn std::error::Error + Send + Sync>,
    },
    ConstraintViolation {
        constraint_name: String,
    },
    InsufficientData {
        required_samples: usize,
        provided_samples: usize,
    },
}
```

### FormatSpecificErrors

Errors for scientific format handlers.

```rust
pub enum VcfError {
    InvalidHeader,
    MalformedVariant { line: usize },
    UnsupportedVersion { version: String },
    AnnotationParseError { field: String },
}

pub enum DicomError {
    InvalidHeader,
    UnsupportedTransferSyntax { syntax: String },
    PixelDataCorrupted,
    ComplianceViolation { message: String },
}

pub enum SamBamError {
    InvalidHeader,
    MalformedAlignment { line: usize },
    UnsupportedFormat { format: DataFormat },
    QualityStringMismatch,
}
```

## Usage Examples

### Complete ML Optimization Workflow

```rust
use stegano::core::*;
use stegano::ml::optimization::*;
use stegano::handlers::vcf::*;
use std::collections::HashMap;

async fn complete_workflow_example() -> Result<()> {
    // Configure optimization
    let mut config = OptimizationConfig::default();
    config.algorithm = OptimizationAlgorithm::BayesianOptimization {
        acquisition_function: AcquisitionFunction::ExpectedImprovement,
    };
    config.max_iterations = 50;
    config.multi_objective.enabled = true;
    
    // Create optimization engine
    let mut optimizer = OptimizationEngine::new(config);
    
    // Add objectives
    optimizer.add_objective(Box::new(GenomicsIntegrityObjective { weight: 1.2 }));
    optimizer.add_objective(Box::new(StealthObjective { weight: 1.0 }));
    optimizer.add_objective(Box::new(CapacityObjective { weight: 0.8 }));
    
    // Add constraints
    optimizer.add_constraint(Box::new(AlleleFrequencyConstraint { min_maf: 0.01 }));
    optimizer.add_constraint(Box::new(MemoryConstraint { max_memory_mb: 1024 }));
    
    // Load genomics data
    let vcf_data = std::fs::read("cohort_study.vcf")?;
    let carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    let payload = b"research_metadata_encrypted";
    
    // Run optimization
    let result = optimizer.multi_objective_optimization(&carrier, payload).await?;
    
    // Analyze results
    println!("Found {} Pareto-optimal solutions", result.pareto_front.len());
    
    // Select best compromise solution
    let best_solution = result.pareto_front.iter()
        .max_by(|a, b| a.1.combined_score.partial_cmp(&b.1.combined_score).unwrap())
        .unwrap();
    
    // Use optimized parameters for actual embedding
    let handler = VcfHandler::new();
    let mut carrier_mut = carrier;
    let embed_result = handler.embed(&mut carrier_mut, payload, &best_solution.0).await?;
    
    println!("Optimized embedding: {} bytes embedded", embed_result.bytes_embedded);
    
    Ok(())
}
```

### Scientific Format Handler Usage

```rust
async fn scientific_formats_example() -> Result<()> {
    // VCF genomics example
    let vcf_handler = VcfHandler::new();
    let vcf_data = std::fs::read("variants.vcf")?;
    let vcf_carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    
    let vcf_params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert("vcf_strategy".to_string(), 
                      serde_json::Value::String("info_fields".to_string()));
            map.insert("preserve_critical_annotations".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"genomics_key".to_vec(),
        use_gpu: false,
    };
    
    let mut vcf_carrier_mut = vcf_carrier;
    let vcf_result = vcf_handler.embed(&mut vcf_carrier_mut, b"genomics_secret", &vcf_params).await?;
    println!("VCF embedding: {} bytes", vcf_result.bytes_embedded);
    
    // DICOM medical imaging example
    let dicom_handler = DicomHandler::new();
    let dicom_data = std::fs::read("scan.dcm")?;
    let dicom_carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    
    let dicom_params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert("dicom_strategy".to_string(), 
                      serde_json::Value::String("private_elements".to_string()));
            map.insert("preserve_clinical_data".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::High,
        encryption_key: b"medical_key".to_vec(),
        use_gpu: false,
    };
    
    let mut dicom_carrier_mut = dicom_carrier;
    let dicom_result = dicom_handler.embed(&mut dicom_carrier_mut, b"medical_secret", &dicom_params).await?;
    println!("DICOM embedding: {} bytes", dicom_result.bytes_embedded);
    
    // SAM/BAM bioinformatics example
    let sam_handler = SamBamHandler::new();
    let sam_data = std::fs::read("alignments.sam")?;
    let sam_carrier = CarrierData::new(sam_data, DataFormat::SAM);
    
    let sam_params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert("sam_strategy".to_string(), 
                      serde_json::Value::String("optional_fields".to_string()));
            map.insert("preserve_core_alignment".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"bioinformatics_key".to_vec(),
        use_gpu: false,
    };
    
    let mut sam_carrier_mut = sam_carrier;
    let sam_result = sam_handler.embed(&mut sam_carrier_mut, b"alignment_secret", &sam_params).await?;
    println!("SAM embedding: {} bytes", sam_result.bytes_embedded);
    
    Ok(())
}
```

### Custom Objective Function

```rust
use stegano::ml::optimization::ObjectiveFunction;

struct CustomDomainObjective {
    pub weight: f32,
    pub domain_specific_params: HashMap<String, f64>,
}

impl ObjectiveFunction for CustomDomainObjective {
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, payload: &[u8]) -> Result<f32> {
        // Implement domain-specific evaluation logic
        let base_score = 0.5;
        
        // Factor in domain-specific considerations
        let domain_factor = self.domain_specific_params
            .get("importance_factor")
            .unwrap_or(&1.0);
        
        // Consider embedding parameters
        let stealth_bonus = match params.stealth_level {
            StealthLevel::Maximum => 0.2,
            StealthLevel::High => 0.15,
            StealthLevel::Medium => 0.1,
            StealthLevel::Low => 0.05,
        };
        
        // Calculate final score
        let final_score = (base_score + stealth_bonus) * (*domain_factor as f32) * self.weight;
        
        Ok(final_score.min(1.0))
    }
    
    fn name(&self) -> &str {
        "custom_domain_objective"
    }
    
    fn maximize(&self) -> bool {
        true
    }
    
    fn weight(&self) -> f32 {
        self.weight
    }
}

// Usage
async fn custom_objective_example() -> Result<()> {
    let mut optimizer = OptimizationEngine::new(OptimizationConfig::default());
    
    let custom_objective = CustomDomainObjective {
        weight: 1.5,
        domain_specific_params: {
            let mut params = HashMap::new();
            params.insert("importance_factor".to_string(), 1.2);
            params.insert("compliance_weight".to_string(), 0.8);
            params
        },
    };
    
    optimizer.add_objective(Box::new(custom_objective));
    
    // Continue with optimization...
    Ok(())
}
```

---

This API reference provides comprehensive documentation for all new ML optimization and scientific format handling capabilities introduced in Stegano v0.2.0. For more detailed examples and usage patterns, refer to the ML Optimization Guide and Scientific Formats Guide.