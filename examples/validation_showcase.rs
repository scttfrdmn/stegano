//! Shows the full statistical validation suite (steganalysis tests, ROC/AUC, cross-validation).

use std::time::Instant;
use stegano::core::*;
use stegano::validation::*;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🎯 Stegano Statistical Validation Showcase");
    println!("==========================================\n");

    // Initialize the comprehensive validation suite
    let validation_suite = StatisticalValidationSuite::new();
    println!("✅ Statistical validation suite initialized\n");

    // Create test data
    let original_data = create_test_image(512, 512);
    let stego_data = create_modified_image(512, 512);
    println!(
        "📊 Created test datasets: 512x512 images ({} bytes each)",
        original_data.len()
    );

    // 1. Classical Statistical Tests
    println!("\n🧮 Classical Statistical Tests");
    println!("-----------------------------");

    let original_f64: Vec<f64> = original_data.iter().map(|&x| x as f64).collect();
    let stego_f64: Vec<f64> = stego_data.iter().map(|&x| x as f64).collect();

    // Chi-square test - create proper frequency distributions
    let start = Instant::now();
    // Convert to histograms for chi-square test
    let (observed_hist, expected_hist) = create_histograms(&stego_f64, &original_f64);
    let chi_square_result = validation_suite
        .classical_tests
        .chi_square_test(&observed_hist, &expected_hist)?;
    println!(
        "• Chi-square test: χ² = {:.3}, p-value = {:.3}, significant = {} ({:.2}ms)",
        chi_square_result.test_statistic,
        chi_square_result.p_value,
        chi_square_result.is_significant,
        start.elapsed().as_millis()
    );

    // Kolmogorov-Smirnov test
    let start = Instant::now();
    let ks_result = validation_suite
        .classical_tests
        .kolmogorov_smirnov_test(&original_f64, &stego_f64)?;
    println!(
        "• KS test: D = {:.3}, p-value = {:.3}, significant = {} ({:.2}ms)",
        ks_result.test_statistic,
        ks_result.p_value,
        ks_result.is_significant,
        start.elapsed().as_millis()
    );

    // Runs test for randomness
    let binary_sequence: Vec<bool> = stego_data.iter().map(|&x| x & 1 == 1).collect();
    let start = Instant::now();
    let runs_result = validation_suite
        .classical_tests
        .runs_test(&binary_sequence)?;
    println!(
        "• Runs test: observed = {}, expected = {:.1}, random = {} ({:.2}ms)",
        runs_result.observed_runs,
        runs_result.expected_runs,
        runs_result.is_random,
        start.elapsed().as_millis()
    );

    // Anderson-Darling normality test
    let start = Instant::now();
    let ad_result = validation_suite
        .classical_tests
        .anderson_darling_test(&stego_f64)?;
    println!(
        "• Anderson-Darling test: A² = {:.3}, p-value = {:.3}, normal = {} ({:.2}ms)",
        ad_result.test_statistic,
        ad_result.p_value,
        ad_result.is_normal,
        start.elapsed().as_millis()
    );

    // 2. Advanced Steganalysis
    println!("\n🔍 Advanced Steganalysis Detection");
    println!("----------------------------------");

    // Pixel pair analysis
    let start = Instant::now();
    let pixel_analysis =
        validation_suite
            .steganalysis
            .pixel_pair_analysis(&stego_data, 512, 512)?;
    println!("• Pixel pair analysis: embedding detected = {}, probability = {:.3}, confidence = {:.3} ({:.2}ms)", 
             pixel_analysis.lsb_embedding_detected,
             pixel_analysis.embedding_probability,
             pixel_analysis.confidence,
             start.elapsed().as_millis());

    // Wavelet structural analysis
    let start = Instant::now();
    let wavelet_analysis = validation_suite
        .steganalysis
        .wavelet_structural_analysis(&stego_data)?;
    println!("• Wavelet analysis: structural changes = {}, anomaly score = {:.3}, affected subbands = {} ({:.2}ms)", 
             wavelet_analysis.structural_changes_detected,
             wavelet_analysis.anomaly_score,
             wavelet_analysis.affected_subbands.len(),
             start.elapsed().as_millis());

    // Audio steganalysis demonstration
    let audio_data = create_test_audio();
    let start = Instant::now();
    let audio_analysis = validation_suite
        .steganalysis
        .audio_steganalysis(&audio_data, 44100.0)?;
    println!(
        "• Audio analysis: spectral anomalies = {}, embedding probability = {:.3} ({:.2}ms)",
        audio_analysis.spectral_anomalies,
        audio_analysis.embedding_probability,
        start.elapsed().as_millis()
    );

    // 3. ROC Analysis & Performance Metrics
    println!("\n📈 ROC Analysis & Performance Metrics");
    println!("------------------------------------");

    // Generate test classification data
    let (true_labels, predicted_scores) = generate_classification_data(1000);
    let start = Instant::now();
    let roc_analysis = validation_suite
        .performance_analysis
        .roc_analysis(&true_labels, &predicted_scores)?;
    println!(
        "• ROC Analysis: AUC = {:.3}, optimal threshold = {:.3}, Youden index = {:.3} ({:.2}ms)",
        roc_analysis.auc,
        roc_analysis.optimal_threshold,
        roc_analysis.youden_index,
        start.elapsed().as_millis()
    );
    println!(
        "  Sensitivity = {:.3}, Specificity = {:.3}",
        roc_analysis.optimal_sensitivity, roc_analysis.optimal_specificity
    );

    // 4. Cross-Validation Framework
    println!("\n🔄 Cross-Validation Framework");
    println!("-----------------------------");

    // Generate dataset for cross-validation
    let cv_data = generate_cv_dataset(500);
    let start = Instant::now();

    // Simple dummy model for demonstration — returns predicted probability scores
    let model_fn =
        |_train_data: &[(Vec<f64>, bool)], test_data: &[(Vec<f64>, bool)]| -> Result<Vec<f64>> {
            // Use first feature directly as a probability score
            let scores: Vec<f64> = test_data.iter().map(|(features, _)| features[0]).collect();
            Ok(scores)
        };

    let cv_result = validation_suite
        .performance_analysis
        .cross_validation
        .k_fold_validation(&cv_data, model_fn)?;
    println!(
        "• 5-fold CV: accuracy = {:.3} ± {:.3}, confidence interval = [{:.3}, {:.3}] ({:.2}ms)",
        cv_result.mean_accuracy,
        cv_result.std_accuracy,
        cv_result.confidence_interval.0,
        cv_result.confidence_interval.1,
        start.elapsed().as_millis()
    );

    // 5. Comprehensive Validation Report
    println!("\n📋 Comprehensive Validation Report");
    println!("----------------------------------");

    let start = Instant::now();
    let validation_report =
        validation_suite.comprehensive_validation(&original_data, &stego_data, &DataFormat::PNG)?;
    println!(
        "• Complete validation: overall score = {:.3} ({:.2}ms)",
        validation_report.overall_score,
        start.elapsed().as_millis()
    );

    // 6. Test Dataset Management
    println!("\n📁 Test Dataset Management");
    println!("-------------------------");

    let mut dataset_manager = TestDatasetManager::new();
    let start = Instant::now();
    dataset_manager.load_standard_datasets()?;
    let datasets = dataset_manager.list_datasets();
    println!(
        "• Loaded {} standard datasets: {:?} ({:.2}ms)",
        datasets.len(),
        datasets,
        start.elapsed().as_millis()
    );

    // Performance summary
    println!("\n🚀 Performance Summary");
    println!("=====================");
    println!("✨ All statistical validation tests completed successfully!");
    println!("📊 Classical tests: Chi-square, KS, Runs, Anderson-Darling");
    println!("🔍 Advanced steganalysis: Pixel pairs, Wavelets, Audio analysis");
    println!("📈 Performance analysis: ROC curves, Cross-validation");
    println!("🎯 Domain-specific validation for scientific data formats");
    println!("📋 Validation reports with per-test metrics");

    println!("\nValidation showcase complete.");

    Ok(())
}

/// Create a test image with gradient pattern
fn create_test_image(width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let value = ((x + y) * 255 / (width + height)) as u8;
            data.push(value);
        }
    }
    data
}

/// Create a slightly modified image for testing
fn create_modified_image(width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let base_value = ((x + y) * 255 / (width + height)) as u8;
            let noise = ((x * y * 7) % 16) as u8;
            data.push(base_value.saturating_add(noise / 4));
        }
    }
    data
}

/// Create one second of test audio (audio_steganalysis needs at least `sample_rate` samples)
fn create_test_audio() -> Vec<f32> {
    (0..44_100)
        .map(|i| {
            let t = i as f32 / 44100.0;
            0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
                + 0.1 * ((i * 17) % 100) as f32 / 500.0 // Add some noise
        })
        .collect()
}

/// Generate binary classification test data
fn generate_classification_data(size: usize) -> (Vec<bool>, Vec<f64>) {
    let true_labels: Vec<bool> = (0..size)
        .map(|i| (i * 137 + 42) % 3 == 0) // ~33% positive
        .collect();

    let predicted_scores: Vec<f64> = (0..size)
        .map(|i| {
            let base_score = if true_labels[i] { 0.7 } else { 0.3 };
            let noise = ((i * 7919) % 1000) as f64 / 2500.0; // ±0.4 noise
            (base_score + noise - 0.2).clamp(0.0, 1.0)
        })
        .collect();

    (true_labels, predicted_scores)
}

/// Generate cross-validation dataset
fn generate_cv_dataset(size: usize) -> Vec<(Vec<f64>, bool)> {
    (0..size)
        .map(|i| {
            let features = vec![
                (i as f64) / (size as f64),
                ((i * 7) % 100) as f64 / 100.0,
                ((i * 13) % 50) as f64 / 50.0,
            ];
            let label = i % 3 == 0;
            (features, label)
        })
        .collect()
}

/// Create histograms for chi-square test
fn create_histograms(observed_data: &[f64], expected_data: &[f64]) -> (Vec<f64>, Vec<f64>) {
    const BINS: usize = 16;
    let mut observed_hist = vec![0.0; BINS];
    let mut expected_hist = vec![0.0; BINS];

    // Find min and max values for binning
    let min_val = observed_data
        .iter()
        .chain(expected_data.iter())
        .copied()
        .fold(f64::INFINITY, f64::min);
    let max_val = observed_data
        .iter()
        .chain(expected_data.iter())
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let range = max_val - min_val;

    if range == 0.0 {
        // All values are the same, create uniform distribution
        for i in 0..BINS {
            observed_hist[i] = observed_data.len() as f64 / BINS as f64;
            expected_hist[i] = expected_data.len() as f64 / BINS as f64;
        }
        return (observed_hist, expected_hist);
    }

    // Bin the data
    for &value in observed_data {
        let bin = ((value - min_val) / range * (BINS - 1) as f64).floor() as usize;
        let bin = bin.min(BINS - 1);
        observed_hist[bin] += 1.0;
    }

    for &value in expected_data {
        let bin = ((value - min_val) / range * (BINS - 1) as f64).floor() as usize;
        let bin = bin.min(BINS - 1);
        expected_hist[bin] += 1.0;
    }

    // Ensure no zero frequencies (add small constant)
    for i in 0..BINS {
        if expected_hist[i] == 0.0 {
            expected_hist[i] = 1.0;
        }
        if observed_hist[i] == 0.0 {
            observed_hist[i] = 1.0;
        }
    }

    (observed_hist, expected_hist)
}
