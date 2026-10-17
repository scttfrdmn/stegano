//! Shows a fast run of the statistical validation suite on synthetic carriers.

use stegano::core::*;
use stegano::validation::*;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Stegano Statistical Validation - Quick Demo");
    println!("===============================================\n");

    let validation_suite = StatisticalValidationSuite::new();
    println!("✅ Validation suite initialized\n");

    // Test 1: Chi-square test
    println!("🧮 Chi-square goodness of fit test:");
    let observed = vec![10.0, 15.0, 20.0, 25.0, 30.0];
    let expected = vec![12.0, 16.0, 18.0, 24.0, 30.0];
    let chi_result = validation_suite
        .classical_tests
        .chi_square_test(&observed, &expected)?;
    println!(
        "   χ² = {:.3}, p-value = {:.3}, significant = {}",
        chi_result.test_statistic, chi_result.p_value, chi_result.is_significant
    );

    // Test 2: Kolmogorov-Smirnov test
    println!("\n📊 Kolmogorov-Smirnov distribution comparison:");
    let sample1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let sample2 = vec![1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5];
    let ks_result = validation_suite
        .classical_tests
        .kolmogorov_smirnov_test(&sample1, &sample2)?;
    println!(
        "   D = {:.3}, p-value = {:.3}, significant = {}",
        ks_result.test_statistic, ks_result.p_value, ks_result.is_significant
    );

    // Test 3: Runs test for randomness
    println!("\n🎲 Runs test for randomness:");
    let sequence = vec![
        true, false, true, true, false, true, false, false, true, false,
    ];
    let runs_result = validation_suite.classical_tests.runs_test(&sequence)?;
    println!(
        "   Observed runs = {}, Expected = {:.1}, Random = {}",
        runs_result.observed_runs, runs_result.expected_runs, runs_result.is_random
    );

    // Test 4: Pixel pair analysis
    println!("\n🖼️  Pixel pair analysis (LSB detection):");
    let image_data = vec![100u8, 101, 102, 103, 104, 105, 106, 107, 108, 109]; // Small test image
    let pixel_result = validation_suite
        .steganalysis
        .pixel_pair_analysis(&image_data, 5, 2)?;
    println!(
        "   LSB embedding detected = {}, Probability = {:.3}",
        pixel_result.lsb_embedding_detected, pixel_result.embedding_probability
    );

    // Test 5: ROC analysis
    println!("\n📈 ROC analysis for binary classification:");
    let true_labels = vec![true, false, true, false, true, true, false, false];
    let scores = vec![0.8, 0.3, 0.9, 0.2, 0.7, 0.85, 0.1, 0.4];
    let roc_result = validation_suite
        .performance_analysis
        .roc_analysis(&true_labels, &scores)?;
    println!(
        "   AUC = {:.3}, Optimal threshold = {:.3}, Youden index = {:.3}",
        roc_result.auc, roc_result.optimal_threshold, roc_result.youden_index
    );

    // Test 6: Test dataset creation
    println!("\n📁 Test dataset management:");
    let mut dataset_manager = TestDatasetManager::new();
    dataset_manager.load_standard_datasets()?;
    let datasets = dataset_manager.list_datasets();
    println!("   Loaded {} datasets: {:?}", datasets.len(), datasets);

    println!("\n🎉 All statistical validation tests completed successfully!");
    println!("Quick validation demo complete.");

    Ok(())
}
