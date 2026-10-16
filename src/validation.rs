//! Comprehensive Statistical Validation Framework
//!
//! This module provides world-class statistical validation capabilities for steganography,
//! including classical statistical tests, advanced steganalysis, and publication-ready metrics.

use crate::core::*;
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Complete statistical validation suite with comprehensive test coverage
#[derive(Debug)]
pub struct StatisticalValidationSuite {
    /// Classical statistical test implementations
    pub classical_tests: ClassicalStatisticalTests,
    /// Advanced steganalysis detection methods
    pub steganalysis: AdvancedSteganalysis,
    /// ROC analysis and performance metrics
    pub performance_analysis: PerformanceAnalysis,
    /// Domain-specific validation for scientific data
    pub domain_validation: DomainSpecificValidation,
    /// Standardized test datasets
    pub test_datasets: TestDatasetManager,
}

/// Classical Statistical Tests Implementation
#[derive(Debug)]
pub struct ClassicalStatisticalTests {
    confidence_level: f64,
}

/// Results of chi-square goodness of fit test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChiSquareResult {
    pub test_statistic: f64,
    pub degrees_of_freedom: usize,
    pub p_value: f64,
    pub critical_value: f64,
    pub is_significant: bool,
    pub effect_size: f64,
}

/// Results of Kolmogorov-Smirnov test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KSTestResult {
    pub test_statistic: f64,
    pub p_value: f64,
    pub critical_value: f64,
    pub is_significant: bool,
    pub max_difference_location: usize,
}

/// Results of runs test for randomness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunsTestResult {
    pub observed_runs: usize,
    pub expected_runs: f64,
    pub variance: f64,
    pub z_score: f64,
    pub p_value: f64,
    pub is_random: bool,
}

/// Results of Anderson-Darling normality test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndersonDarlingResult {
    pub test_statistic: f64,
    pub p_value: f64,
    pub critical_values: Vec<f64>,
    pub significance_levels: Vec<f64>,
    pub is_normal: bool,
}

/// Autocorrelation analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutocorrelationResult {
    pub correlations: Vec<f64>,
    pub significant_lags: Vec<usize>,
    pub ljung_box_statistic: f64,
    pub ljung_box_p_value: f64,
    pub has_correlation: bool,
}

impl ClassicalStatisticalTests {
    pub fn new(confidence_level: f64) -> Self {
        Self { confidence_level }
    }

    /// Complete chi-square goodness of fit test with p-value calculation
    pub fn chi_square_test(&self, observed: &[f64], expected: &[f64]) -> Result<ChiSquareResult> {
        if observed.len() != expected.len() {
            return Err(SteganoError::InvalidParameter(
                "Observed and expected arrays must have same length".to_string(),
            ));
        }

        if observed.is_empty() {
            return Err(SteganoError::InvalidParameter(
                "Arrays cannot be empty".to_string(),
            ));
        }

        // Calculate chi-square test statistic
        let mut chi_square = 0.0;
        for (obs, exp) in observed.iter().zip(expected.iter()) {
            if *exp <= 0.0 {
                return Err(SteganoError::InvalidParameter(
                    "Expected frequencies must be positive".to_string(),
                ));
            }
            chi_square += (obs - exp).powi(2) / exp;
        }

        let df = observed.len() - 1;
        let p_value = self.chi_square_p_value(chi_square, df);
        let critical_value = self.chi_square_critical_value(df, self.confidence_level);

        // Calculate effect size (Cramér's V for goodness of fit)
        let n: f64 = observed.iter().sum();
        let effect_size = (chi_square / (n * (observed.len() - 1) as f64)).sqrt();

        Ok(ChiSquareResult {
            test_statistic: chi_square,
            degrees_of_freedom: df,
            p_value,
            critical_value,
            is_significant: p_value < (1.0 - self.confidence_level),
            effect_size,
        })
    }

    /// Two-sample Kolmogorov-Smirnov test for distribution comparison
    pub fn kolmogorov_smirnov_test(
        &self,
        sample1: &[f64],
        sample2: &[f64],
    ) -> Result<KSTestResult> {
        if sample1.is_empty() || sample2.is_empty() {
            return Err(SteganoError::InvalidParameter(
                "Samples cannot be empty".to_string(),
            ));
        }

        let mut sorted1 = sample1.to_vec();
        let mut sorted2 = sample2.to_vec();
        sorted1.sort_by(|a, b| a.total_cmp(b));
        sorted2.sort_by(|a, b| a.total_cmp(b));

        let n1 = sample1.len() as f64;
        let n2 = sample2.len() as f64;

        // Calculate empirical CDFs and find maximum difference
        let mut max_diff = 0.0;
        let mut max_diff_location = 0;
        let mut i1 = 0;
        let mut i2 = 0;

        // Merge and compare CDFs
        while i1 < sorted1.len() || i2 < sorted2.len() {
            let x1 = if i1 < sorted1.len() {
                sorted1[i1]
            } else {
                f64::INFINITY
            };
            let x2 = if i2 < sorted2.len() {
                sorted2[i2]
            } else {
                f64::INFINITY
            };

            let x = x1.min(x2);

            // Count values <= x in each sample
            let cdf1 = sorted1.iter().take_while(|&&val| val <= x).count() as f64 / n1;
            let cdf2 = sorted2.iter().take_while(|&&val| val <= x).count() as f64 / n2;

            let diff = (cdf1 - cdf2).abs();
            if diff > max_diff {
                max_diff = diff;
                max_diff_location = if x1 <= x2 { i1 } else { i2 + sorted1.len() };
            }

            if x1 <= x2 {
                i1 += 1;
            }
            if x2 <= x1 {
                i2 += 1;
            }
        }

        // Calculate critical value and p-value
        let critical_value =
            self.ks_critical_value(n1 as usize, n2 as usize, self.confidence_level);
        let p_value = self.ks_p_value(max_diff, n1 as usize, n2 as usize);

        Ok(KSTestResult {
            test_statistic: max_diff,
            p_value,
            critical_value,
            is_significant: max_diff > critical_value,
            max_difference_location: max_diff_location,
        })
    }

    /// Runs test for randomness assessment
    pub fn runs_test(&self, sequence: &[bool]) -> Result<RunsTestResult> {
        if sequence.len() < 2 {
            return Err(SteganoError::InvalidParameter(
                "Sequence must have at least 2 elements".to_string(),
            ));
        }

        // Count runs
        let mut runs = 1;
        for window in sequence.windows(2) {
            if window[0] != window[1] {
                runs += 1;
            }
        }

        // Count occurrences of each value
        let n1 = sequence.iter().filter(|&&x| x).count() as f64;
        let n2 = sequence.iter().filter(|&&x| !x).count() as f64;
        let n = n1 + n2;

        if n1 == 0.0 || n2 == 0.0 {
            return Err(SteganoError::InvalidParameter(
                "Sequence must contain both true and false values".to_string(),
            ));
        }

        // Calculate expected runs and variance
        let expected_runs = (2.0 * n1 * n2) / n + 1.0;
        let variance = (2.0 * n1 * n2 * (2.0 * n1 * n2 - n)) / (n.powi(2) * (n - 1.0));

        // Calculate z-score
        let z_score = if variance > 0.0 {
            (runs as f64 - expected_runs) / variance.sqrt()
        } else {
            0.0
        };

        // Calculate p-value (two-tailed test)
        let p_value = 2.0 * (1.0 - self.standard_normal_cdf(z_score.abs()));

        Ok(RunsTestResult {
            observed_runs: runs,
            expected_runs,
            variance,
            z_score,
            p_value,
            is_random: p_value > (1.0 - self.confidence_level),
        })
    }

    /// Anderson-Darling test for normality
    pub fn anderson_darling_test(&self, data: &[f64]) -> Result<AndersonDarlingResult> {
        if data.len() < 5 {
            return Err(SteganoError::InvalidParameter(
                "Need at least 5 data points for Anderson-Darling test".to_string(),
            ));
        }

        let mut sorted_data = data.to_vec();
        sorted_data.sort_by(|a, b| a.total_cmp(b));

        let n = data.len() as f64;
        let mean = sorted_data.iter().sum::<f64>() / n;
        let variance = sorted_data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_dev = variance.sqrt();

        // Standardize data
        let standardized: Vec<f64> = sorted_data.iter().map(|x| (x - mean) / std_dev).collect();

        // Calculate Anderson-Darling statistic
        let mut ad_stat = 0.0;
        for (i, &x) in standardized.iter().enumerate() {
            let phi = self.standard_normal_cdf(x);
            let i_f64 = (i + 1) as f64;
            let _n_minus_i = n - i_f64 + 1.0;

            if phi > 0.0 && phi < 1.0 {
                ad_stat += (2.0 * i_f64 - 1.0) * (phi.ln() + (1.0 - phi).ln()) / n;
            }
        }
        ad_stat = -n - ad_stat;

        // Adjust for sample size
        ad_stat *= 1.0 + 4.0 / n - 25.0 / (n * n);

        // Critical values for different significance levels
        let critical_values = vec![0.576, 0.656, 0.787, 0.918, 1.092];
        let significance_levels = vec![0.15, 0.10, 0.05, 0.025, 0.01];

        // Estimate p-value
        let p_value = self.anderson_darling_p_value(ad_stat);

        Ok(AndersonDarlingResult {
            test_statistic: ad_stat,
            p_value,
            critical_values,
            significance_levels,
            is_normal: p_value > (1.0 - self.confidence_level),
        })
    }

    /// Autocorrelation analysis for temporal dependencies
    pub fn autocorrelation_analysis(
        &self,
        data: &[f64],
        max_lag: usize,
    ) -> Result<AutocorrelationResult> {
        if data.len() < max_lag * 2 {
            return Err(SteganoError::InvalidParameter(
                "Data length must be at least twice the maximum lag".to_string(),
            ));
        }

        let n = data.len();
        let mean = data.iter().sum::<f64>() / n as f64;

        // Calculate variance
        let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;

        if variance == 0.0 {
            return Err(SteganoError::InvalidParameter(
                "Data has zero variance".to_string(),
            ));
        }

        // Calculate autocorrelations
        let mut correlations = Vec::with_capacity(max_lag + 1);
        let mut significant_lags = Vec::new();

        for lag in 0..=max_lag {
            let mut covariance = 0.0;
            let valid_pairs = n - lag;

            for i in 0..valid_pairs {
                covariance += (data[i] - mean) * (data[i + lag] - mean);
            }
            covariance /= valid_pairs as f64;

            let correlation = covariance / variance;
            correlations.push(correlation);

            // Check significance (approximate 95% confidence bounds)
            let bound = 1.96 / (n as f64).sqrt();
            if lag > 0 && correlation.abs() > bound {
                significant_lags.push(lag);
            }
        }

        // Ljung-Box test for overall autocorrelation
        let ljung_box_statistic = self.ljung_box_statistic(&correlations[1..], n);
        let ljung_box_p_value = self.chi_square_p_value(ljung_box_statistic, max_lag);

        Ok(AutocorrelationResult {
            correlations,
            significant_lags,
            ljung_box_statistic,
            ljung_box_p_value,
            has_correlation: ljung_box_p_value < (1.0 - self.confidence_level),
        })
    }

    // Helper methods for statistical calculations

    /// Upper-tail probability `P(χ²_df ≥ chi_square)` (exact, via the regularized
    /// incomplete gamma function).
    fn chi_square_p_value(&self, chi_square: f64, df: usize) -> f64 {
        chi_square_survival(chi_square, df)
    }

    /// One-tailed (upper) critical value of the χ² distribution at significance
    /// `α = 1 − confidence`, i.e. the value `c` with `P(χ²_df ≥ c) = α`.
    fn chi_square_critical_value(&self, df: usize, confidence: f64) -> f64 {
        chi_square_upper_quantile(1.0 - confidence, df)
    }

    fn ks_critical_value(&self, n1: usize, n2: usize, confidence: f64) -> f64 {
        let alpha = 1.0 - confidence;
        let c_alpha = if alpha <= 0.01 {
            1.63
        } else if alpha <= 0.05 {
            1.36
        } else {
            1.22
        };

        c_alpha * ((n1 + n2) as f64 / (n1 * n2) as f64).sqrt()
    }

    fn ks_p_value(&self, d_statistic: f64, n1: usize, n2: usize) -> f64 {
        let n_eff = ((n1 * n2) as f64 / (n1 + n2) as f64).sqrt();
        let z = d_statistic * n_eff;

        // Kolmogorov distribution approximation (clamped to valid range)
        let p = 2.0 * (-2.0 * z * z).exp();
        p.clamp(0.0, 1.0)
    }

    fn anderson_darling_p_value(&self, ad_stat: f64) -> f64 {
        // Approximate p-value calculation for Anderson-Darling test
        if ad_stat < 0.2 {
            1.0 - (-1.2337 * ad_stat).exp()
                * (2.00012 + 0.247105 * ad_stat + 0.0649821 * ad_stat.powi(2))
        } else if ad_stat < 0.34 {
            1.0 - (-0.9177 * ad_stat).exp() * (1.92294 + 0.293361 * ad_stat)
        } else if ad_stat < 0.6 {
            (-0.731 * ad_stat + 0.08861).exp()
        } else {
            (-1.386294 * ad_stat + 0.277746).exp()
        }
    }

    fn ljung_box_statistic(&self, autocorrelations: &[f64], n: usize) -> f64 {
        let n_f64 = n as f64;
        let mut statistic = 0.0;

        for (k, &rho) in autocorrelations.iter().enumerate() {
            let lag = k + 1;
            statistic += rho.powi(2) / (n_f64 - lag as f64);
        }

        n_f64 * (n_f64 + 2.0) * statistic
    }

    fn standard_normal_cdf(&self, x: f64) -> f64 {
        // Approximation of the standard normal CDF
        0.5 * (1.0 + self.error_function(x / 2_f64.sqrt()))
    }

    fn error_function(&self, x: f64) -> f64 {
        // Approximation of the error function
        let a1 = 0.254829592;
        let a2 = -0.284496736;
        let a3 = 1.421413741;
        let a4 = -1.453152027;
        let a5 = 1.061405429;
        let p = 0.3275911;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();

        let t = 1.0 / (1.0 + p * x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

        sign * y
    }
}

/// Advanced Steganalysis Detection Methods
#[derive(Debug)]
pub struct AdvancedSteganalysis {
    detection_threshold: f64,
}

/// Pixel pair analysis results for LSB detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelPairAnalysisResult {
    pub lsb_embedding_detected: bool,
    pub embedding_probability: f64,
    pub affected_pairs: usize,
    pub chi_square_statistic: f64,
    pub p_value: f64,
    pub confidence: f64,
}

/// Structural analysis using wavelets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveletAnalysisResult {
    pub structural_changes_detected: bool,
    pub energy_distribution: Vec<f64>,
    pub anomaly_score: f64,
    pub affected_subbands: Vec<usize>,
    pub confidence: f64,
}

/// Audio steganalysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSteganalysisResult {
    pub spectral_anomalies: bool,
    pub phase_coherence_score: f64,
    pub cepstral_analysis_score: f64,
    pub harmonic_distortion: f64,
    pub embedding_probability: f64,
}

impl AdvancedSteganalysis {
    pub fn new(detection_threshold: f64) -> Self {
        Self {
            detection_threshold,
        }
    }

    /// Pixel-pair LSB steganalysis.
    ///
    /// Historically this function computed a statistic that was identically
    /// `0.5 · N` for every input. It is now a thin wrapper around
    /// [`AdvancedSteganalysis::sample_pairs_analysis`] (Dumitrescu–Wu–Wang
    /// Sample Pairs Analysis), which works on pairs of spatially adjacent samples.
    ///
    /// * `image` — raw 8-bit samples in row-major order (not a compressed file).
    /// * `width`, `height` — image dimensions in pixels. If `width * height`
    ///   evenly divides `image.len()` with a quotient of 1–4, that quotient is used
    ///   as the interleaved channel count; otherwise one channel is assumed. If
    ///   `height == 0` or the image is smaller than `width * height`, the data is
    ///   treated as rows of `width` samples.
    ///
    /// Field meanings in the result:
    /// * `embedding_probability` — SPA estimate of the LSB-replacement embedding
    ///   rate (fraction of samples carrying message bits), clamped to `[0, 1]`.
    /// * `affected_pairs` — `round(embedding_probability × pairs analysed)`.
    /// * `chi_square_statistic` / `p_value` — the SPA balance test
    ///   `(|X| − |Y|)² / (|X| + |Y|)` with 1 degree of freedom. For a clean cover
    ///   SPA assumes `|X| ≈ |Y|`; LSB replacement breaks that balance, so a small
    ///   `p_value` is evidence of embedding.
    /// * `confidence` — `1 − p_value`.
    /// * `lsb_embedding_detected` — estimated rate exceeds the configured
    ///   detection threshold **and** the balance test rejects at α = 0.05.
    pub fn pixel_pair_analysis(
        &self,
        image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<PixelPairAnalysisResult> {
        if image.is_empty() || width == 0 {
            return Err(SteganoError::InvalidParameter(
                "Invalid image parameters".to_string(),
            ));
        }

        let pixels = width.saturating_mul(height);
        let (samples, channels, rows) = if height > 0 && pixels > 0 && image.len() >= pixels {
            let ch = image.len() / pixels;
            if image.len().is_multiple_of(pixels) && (1..=4).contains(&ch) {
                (&image[..pixels * ch], ch, height)
            } else {
                (&image[..pixels], 1, height)
            }
        } else {
            (image, 1, image.len().div_ceil(width))
        };

        if samples.len() < 2 {
            return Ok(PixelPairAnalysisResult {
                lsb_embedding_detected: false,
                embedding_probability: 0.0,
                affected_pairs: 0,
                chi_square_statistic: 0.0,
                p_value: 1.0,
                confidence: 0.0,
            });
        }

        let spa = self.sample_pairs_analysis(samples, width, rows, channels)?;
        let affected_pairs = (spa.estimated_rate * spa.pairs_analyzed as f64).round() as usize;

        Ok(PixelPairAnalysisResult {
            lsb_embedding_detected: spa.embedding_detected,
            embedding_probability: spa.estimated_rate,
            affected_pairs,
            chi_square_statistic: spa.balance_chi_square,
            p_value: spa.balance_p_value,
            confidence: 1.0 - spa.balance_p_value,
        })
    }

    /// Westfeld–Pfitzmann pairs-of-values (PoV) χ² attack on LSB replacement.
    ///
    /// Reference: A. Westfeld and A. Pfitzmann, "Attacks on Steganographic
    /// Systems", *Information Hiding 1999*, LNCS 1768, pp. 61–76.
    ///
    /// LSB replacement with uniformly random message bits equalises the
    /// histogram counts within every pair of values `(2k, 2k+1)`. The test is a
    /// Pearson goodness-of-fit against that *stego* hypothesis:
    ///
    /// `χ² = Σ_k (h[2k] − h[2k+1])² / (h[2k] + h[2k+1])`
    ///
    /// which is the full two-cell Pearson sum for each pair (Westfeld's paper
    /// sums only the even cell, which halves the statistic). Pairs with fewer
    /// than 10 total samples (expected count < 5 per cell) are skipped. Under the
    /// stego hypothesis the statistic is asymptotically χ² with one degree of
    /// freedom per pair used.
    ///
    /// `embedding_probability = P(χ²_df ≥ statistic)` is Westfeld's "probability
    /// of embedding": it is roughly uniform on `(0, 1)` for a fully embedded
    /// carrier and collapses towards 0 for a cover whose histogram has
    /// pair-asymmetric structure. The attack is blind to covers whose histogram
    /// is already smooth at the 1-LSB scale (such covers look "embedded" to PoV);
    /// use [`Self::sample_pairs_analysis`] for those. For sequential embedding,
    /// apply the test to increasing prefixes of the carrier as Westfeld does; the
    /// probability stays high while the prefix lies inside the embedded region.
    ///
    /// `embedding_detected` is true when the stego hypothesis is *not* rejected
    /// at α = 0.05 (`embedding_probability > 0.05`) and at least two pairs were
    /// used.
    pub fn pairs_of_values_test(&self, samples: &[u8]) -> Result<PairsOfValuesResult> {
        if samples.is_empty() {
            return Err(SteganoError::InvalidParameter(
                "Cannot run pairs-of-values test on empty data".to_string(),
            ));
        }
        let mut hist = [0u64; 256];
        for &b in samples {
            hist[b as usize] += 1;
        }

        let mut chi_square = 0.0;
        let mut pairs_used = 0usize;
        for &[even, odd] in hist.as_chunks::<2>().0 {
            let (even, odd) = (even as f64, odd as f64);
            let total = even + odd;
            if total < POV_MIN_PAIR_COUNT {
                continue;
            }
            chi_square += (even - odd).powi(2) / total;
            pairs_used += 1;
        }

        let embedding_probability = if pairs_used == 0 {
            0.0
        } else {
            chi_square_survival(chi_square, pairs_used)
        };

        Ok(PairsOfValuesResult {
            chi_square_statistic: chi_square,
            degrees_of_freedom: pairs_used,
            embedding_probability,
            pairs_used,
            embedding_detected: pairs_used >= 2 && embedding_probability > DETECTOR_ALPHA,
        })
    }

    /// Sample Pairs Analysis (SPA) estimate of the LSB-replacement embedding rate.
    ///
    /// Reference: S. Dumitrescu, X. Wu and Z. Wang, "Detection of LSB
    /// steganography via sample pair analysis", *IEEE Trans. Signal Processing*
    /// 51(7), 2003, pp. 1995–2007.
    ///
    /// Each pair `(u, v)` of adjacent samples (horizontal neighbours `channels`
    /// bytes apart within a row of `width · channels` bytes, plus vertical
    /// neighbours when `height > 1`) is classified into:
    ///
    /// * `X`: `v` even and `u < v`, or `v` odd and `u > v`;
    /// * `Y`: `v` even and `u > v`, or `v` odd and `u < v`;
    /// * `K = W ∪ Z`: `⌊u/2⌋ = ⌊v/2⌋` (the closed set `C₀`).
    ///
    /// Under the SPA cover model (`|X₂ₘ₊₁| = |Y₂ₘ₊₁|`), the embedding rate `p`
    /// (fraction of samples carrying message bits — about half of which flip)
    /// is the smaller root of
    ///
    /// `(|K| / 2) p² + (2|X| − |P|) p + (|Y| − |X|) = 0`.
    ///
    /// `estimated_rate` is that root clamped to `[0, 1]`; `raw_estimate` is the
    /// unclamped root (useful as a continuous detector score). A clean natural
    /// cover typically yields `|raw_estimate|` of a few percent; 100 % embedding
    /// yields ≈ 1. The balance test `(|X| − |Y|)² / (|X| + |Y|) ~ χ²₁` gives a
    /// p-value for the clean-cover hypothesis `|X| = |Y|`.
    ///
    /// For 1-D signals pass `height = 1` and `width = samples.len()`.
    pub fn sample_pairs_analysis(
        &self,
        samples: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Result<SamplePairsResult> {
        if samples.len() < 2 || width == 0 || channels == 0 {
            return Err(SteganoError::InvalidParameter(
                "Sample pairs analysis needs at least two samples and non-zero width/channels"
                    .to_string(),
            ));
        }
        let counts = spa_counts(samples, width, height, channels);
        Ok(self.spa_from_counts(&counts))
    }

    fn spa_from_counts(&self, c: &SpaCounts) -> SamplePairsResult {
        let raw = spa_rate(c);
        let raw = if raw.is_finite() { raw } else { 0.0 };
        let estimated_rate = raw.clamp(0.0, 1.0);
        let (balance_chi_square, balance_p_value) = if c.x + c.y > 0 {
            let d = c.x as f64 - c.y as f64;
            let chi = d * d / (c.x + c.y) as f64;
            (chi, chi_square_survival(chi, 1))
        } else {
            (0.0, 1.0)
        };
        SamplePairsResult {
            estimated_rate,
            raw_estimate: raw,
            pairs_analyzed: c.pairs,
            x_pairs: c.x,
            y_pairs: c.y,
            closed_pairs: c.k,
            balance_chi_square,
            balance_p_value,
            embedding_detected: estimated_rate > self.detection_threshold
                && balance_p_value < DETECTOR_ALPHA,
        }
    }

    /// Wavelet-based structural analysis for advanced steganalysis
    pub fn wavelet_structural_analysis(&self, data: &[u8]) -> Result<WaveletAnalysisResult> {
        if data.len() < 64 {
            return Err(SteganoError::InvalidParameter(
                "Data too small for wavelet analysis".to_string(),
            ));
        }

        // Simple Haar wavelet transform for demonstration
        let float_data: Vec<f64> = data.iter().map(|&x| x as f64).collect();
        let (low_freq, high_freq) = self.haar_wavelet_transform(&float_data)?;

        // Analyze energy distribution
        let total_energy: f64 = float_data.iter().map(|x| x * x).sum();
        let low_energy: f64 = low_freq.iter().map(|x| x * x).sum();
        let high_energy: f64 = high_freq.iter().map(|x| x * x).sum();

        let energy_distribution = vec![low_energy / total_energy, high_energy / total_energy];

        // Detect anomalies in high-frequency components
        let high_freq_mean = high_freq.iter().sum::<f64>() / high_freq.len() as f64;
        let high_freq_variance = high_freq
            .iter()
            .map(|x| (x - high_freq_mean).powi(2))
            .sum::<f64>()
            / high_freq.len() as f64;

        // Anomaly score based on unexpected high-frequency energy
        let expected_high_freq_ratio = 0.3; // Expected ratio for natural data
        let observed_high_freq_ratio = high_energy / total_energy;
        let anomaly_score = (observed_high_freq_ratio - expected_high_freq_ratio).abs();

        // Identify affected subbands
        let mut affected_subbands = Vec::new();
        let threshold = high_freq_mean + 2.0 * high_freq_variance.sqrt();
        for (i, &coeff) in high_freq.iter().enumerate() {
            if coeff.abs() > threshold {
                affected_subbands.push(i);
            }
        }

        let structural_changes_detected = anomaly_score > self.detection_threshold;
        let confidence = anomaly_score.min(1.0);

        Ok(WaveletAnalysisResult {
            structural_changes_detected,
            energy_distribution,
            anomaly_score,
            affected_subbands,
            confidence,
        })
    }

    /// Comprehensive audio steganalysis
    pub fn audio_steganalysis(
        &self,
        audio_data: &[f32],
        sample_rate: f32,
    ) -> Result<AudioSteganalysisResult> {
        if audio_data.len() < sample_rate as usize {
            return Err(SteganoError::InvalidParameter(
                "Audio data too short for analysis".to_string(),
            ));
        }

        // Spectral analysis
        let spectral_score = self.spectral_anomaly_analysis(audio_data)?;

        // Phase coherence analysis
        let phase_coherence_score = self.phase_coherence_analysis(audio_data)?;

        // Cepstral analysis
        let cepstral_score = self.cepstral_analysis(audio_data)?;

        // Harmonic distortion analysis
        let harmonic_distortion = self.harmonic_distortion_analysis(audio_data, sample_rate)?;

        // Combined embedding probability
        let embedding_probability =
            (spectral_score + phase_coherence_score + cepstral_score + harmonic_distortion) / 4.0;

        let spectral_anomalies = embedding_probability > self.detection_threshold;

        Ok(AudioSteganalysisResult {
            spectral_anomalies,
            phase_coherence_score,
            cepstral_analysis_score: cepstral_score,
            harmonic_distortion,
            embedding_probability,
        })
    }

    // Helper methods for wavelet and audio analysis

    fn haar_wavelet_transform(&self, data: &[f64]) -> Result<(Vec<f64>, Vec<f64>)> {
        if !data.len().is_multiple_of(2) {
            return Err(SteganoError::InvalidParameter(
                "Data length must be even for Haar wavelet".to_string(),
            ));
        }

        let mut low_freq = Vec::with_capacity(data.len() / 2);
        let mut high_freq = Vec::with_capacity(data.len() / 2);

        for i in (0..data.len()).step_by(2) {
            let avg = (data[i] + data[i + 1]) / 2.0; // Low frequency (approximation)
            let diff = (data[i] - data[i + 1]) / 2.0; // High frequency (detail)

            low_freq.push(avg);
            high_freq.push(diff);
        }

        Ok((low_freq, high_freq))
    }

    fn spectral_anomaly_analysis(&self, audio_data: &[f32]) -> Result<f64> {
        // Simple FFT-based spectral analysis
        let window_size = 1024.min(audio_data.len());
        let mut spectral_energy = vec![0.0; window_size / 2];

        // Process overlapping windows
        for window_start in
            (0..audio_data.len().saturating_sub(window_size)).step_by(window_size / 2)
        {
            let window = &audio_data[window_start..window_start + window_size];
            let fft_result = self.simple_fft(window)?;

            for (i, &magnitude) in fft_result.iter().enumerate() {
                if i < spectral_energy.len() {
                    spectral_energy[i] += magnitude * magnitude;
                }
            }
        }

        // Detect anomalies in spectral distribution
        let total_energy: f64 = spectral_energy.iter().map(|&x| x as f64).sum();
        let high_freq_energy: f64 = spectral_energy[spectral_energy.len() / 2..]
            .iter()
            .map(|&x| x as f64)
            .sum();
        let high_freq_ratio = high_freq_energy / total_energy;

        // Anomaly score based on unexpected high-frequency content
        Ok((high_freq_ratio - 0.1).clamp(0.0, 1.0)) // Expected ratio ~0.1 for speech
    }

    fn phase_coherence_analysis(&self, audio_data: &[f32]) -> Result<f64> {
        // Analyze phase relationships between adjacent samples
        let mut phase_jumps = 0;
        let mut total_comparisons = 0;

        for window in audio_data.windows(3) {
            let phase1 = (window[1] / window[0]).atan();
            let phase2 = (window[2] / window[1]).atan();
            let phase_diff = (phase2 - phase1).abs();

            if phase_diff > std::f32::consts::PI / 2.0 {
                phase_jumps += 1;
            }
            total_comparisons += 1;
        }

        let phase_jump_ratio = phase_jumps as f64 / total_comparisons as f64;
        Ok(phase_jump_ratio)
    }

    fn cepstral_analysis(&self, audio_data: &[f32]) -> Result<f64> {
        // Simple cepstral analysis for detecting embedding artifacts
        // Use a fixed window to keep O(n^2) DFT tractable
        let window_size = 1024.min(audio_data.len());
        let fft_result = self.simple_fft(&audio_data[..window_size])?;

        // Log magnitude spectrum
        let log_spectrum: Vec<f32> = fft_result.iter().map(|&x| (x + 1e-10).ln()).collect();

        // IFFT of log spectrum (simplified cepstrum)
        let cepstrum = self.simple_fft(&log_spectrum)?;

        // Analyze cepstral coefficients for anomalies
        let cepstral_variance =
            cepstrum.iter().map(|&x| x * x).sum::<f32>() / cepstrum.len() as f32;

        Ok(((cepstral_variance - 0.1).max(0.0) as f64).min(1.0))
    }

    fn harmonic_distortion_analysis(&self, audio_data: &[f32], _sample_rate: f32) -> Result<f64> {
        let window_size = 1024.min(audio_data.len());
        let fft_result = self.simple_fft(&audio_data[..window_size])?;

        // Find fundamental frequency (simplified)
        let mut max_magnitude = 0.0;
        let mut fundamental_bin = 0;

        for (i, &magnitude) in fft_result.iter().enumerate().take(fft_result.len() / 4) {
            if magnitude > max_magnitude {
                max_magnitude = magnitude;
                fundamental_bin = i;
            }
        }

        if fundamental_bin == 0 {
            return Ok(0.0);
        }

        // Check harmonics
        let mut harmonic_energy = 0.0;
        let mut total_energy = max_magnitude * max_magnitude;

        for harmonic in 2..=5 {
            let harmonic_bin = fundamental_bin * harmonic;
            if harmonic_bin < fft_result.len() {
                let harmonic_magnitude = fft_result[harmonic_bin];
                harmonic_energy += harmonic_magnitude * harmonic_magnitude;
                total_energy += harmonic_magnitude * harmonic_magnitude;
            }
        }

        let thd = if total_energy > 0.0 {
            (harmonic_energy / total_energy).sqrt()
        } else {
            0.0
        };

        Ok((thd as f64).min(1.0))
    }

    fn simple_fft(&self, data: &[f32]) -> Result<Vec<f32>> {
        let n = data.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);

        let mut buffer: Vec<Complex<f32>> =
            data.iter().map(|&x| Complex { re: x, im: 0.0 }).collect();

        fft.process(&mut buffer);

        let magnitudes: Vec<f32> = buffer.iter().map(|c| c.norm()).collect();
        Ok(magnitudes)
    }
}

/// ROC Analysis and Performance Metrics
#[derive(Debug)]
pub struct PerformanceAnalysis {
    pub roc_analysis: ROCAnalysis,
    pub cross_validation: CrossValidation,
}

/// ROC Analysis Results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ROCAnalysis {
    pub true_positive_rates: Vec<f64>,
    pub false_positive_rates: Vec<f64>,
    pub thresholds: Vec<f64>,
    pub auc: f64,
    pub optimal_threshold: f64,
    pub optimal_sensitivity: f64,
    pub optimal_specificity: f64,
    pub youden_index: f64,
}

/// Cross-validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossValidationResult {
    pub fold_accuracies: Vec<f64>,
    pub mean_accuracy: f64,
    pub std_accuracy: f64,
    pub confidence_interval: (f64, f64),
    pub fold_aucs: Vec<f64>,
    pub mean_auc: f64,
}

/// K-fold cross-validation framework
#[derive(Debug)]
pub struct CrossValidation {
    k_folds: usize,
    #[allow(dead_code)]
    random_seed: u64,
}

impl PerformanceAnalysis {
    pub fn new() -> Self {
        Self {
            roc_analysis: ROCAnalysis::new(),
            cross_validation: CrossValidation::new(5, 42),
        }
    }

    /// Comprehensive ROC analysis with optimal threshold selection
    pub fn roc_analysis(
        &self,
        true_labels: &[bool],
        predicted_scores: &[f64],
    ) -> Result<ROCAnalysis> {
        if true_labels.len() != predicted_scores.len() {
            return Err(SteganoError::InvalidParameter(
                "Labels and scores must have same length".to_string(),
            ));
        }

        if true_labels.is_empty() {
            return Err(SteganoError::InvalidParameter(
                "Cannot perform ROC analysis on empty data".to_string(),
            ));
        }

        // Create threshold points
        let mut thresholds: Vec<f64> = predicted_scores.to_vec();
        thresholds.sort_by(|a, b| a.total_cmp(b));
        thresholds.dedup();

        let mut tpr_values = Vec::new();
        let mut fpr_values = Vec::new();
        let mut threshold_values = Vec::new();

        // Calculate TPR and FPR for each threshold
        for &threshold in &thresholds {
            let (tp, fp, tn, fn_count) =
                self.confusion_matrix_at_threshold(true_labels, predicted_scores, threshold);

            let tpr = if tp + fn_count > 0 {
                tp as f64 / (tp + fn_count) as f64
            } else {
                0.0
            };
            let fpr = if fp + tn > 0 {
                fp as f64 / (fp + tn) as f64
            } else {
                0.0
            };

            tpr_values.push(tpr);
            fpr_values.push(fpr);
            threshold_values.push(threshold);
        }

        // AUC = P(score_stego > score_cover) + ½·P(tie) (Mann–Whitney U), which is
        // the exact area under the full empirical ROC curve including its
        // (0,0) and (1,1) endpoints.
        let auc = CrossValidation::mann_whitney_auc(true_labels, predicted_scores);

        // Find optimal threshold using Youden's index
        let (optimal_threshold, optimal_sensitivity, optimal_specificity, youden_index) =
            self.find_optimal_threshold(&threshold_values, &tpr_values, &fpr_values);

        Ok(ROCAnalysis {
            true_positive_rates: tpr_values,
            false_positive_rates: fpr_values,
            thresholds: threshold_values,
            auc,
            optimal_threshold,
            optimal_sensitivity,
            optimal_specificity,
            youden_index,
        })
    }

    fn confusion_matrix_at_threshold(
        &self,
        true_labels: &[bool],
        predicted_scores: &[f64],
        threshold: f64,
    ) -> (usize, usize, usize, usize) {
        let mut tp = 0; // True positives
        let mut fp = 0; // False positives
        let mut tn = 0; // True negatives
        let mut fn_count = 0; // False negatives

        for (&true_label, &score) in true_labels.iter().zip(predicted_scores.iter()) {
            let predicted = score >= threshold;

            match (true_label, predicted) {
                (true, true) => tp += 1,
                (false, true) => fp += 1,
                (false, false) => tn += 1,
                (true, false) => fn_count += 1,
            }
        }

        (tp, fp, tn, fn_count)
    }

    fn find_optimal_threshold(
        &self,
        thresholds: &[f64],
        tpr: &[f64],
        fpr: &[f64],
    ) -> (f64, f64, f64, f64) {
        let mut best_youden = -1.0;
        let mut best_threshold = 0.0;
        let mut best_sensitivity = 0.0;
        let mut best_specificity = 0.0;

        for i in 0..thresholds.len() {
            let sensitivity = tpr[i];
            let specificity = 1.0 - fpr[i];
            let youden_index = sensitivity + specificity - 1.0;

            if youden_index > best_youden {
                best_youden = youden_index;
                best_threshold = thresholds[i];
                best_sensitivity = sensitivity;
                best_specificity = specificity;
            }
        }

        (
            best_threshold,
            best_sensitivity,
            best_specificity,
            best_youden,
        )
    }
}

impl Default for PerformanceAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

impl ROCAnalysis {
    fn new() -> Self {
        Self {
            true_positive_rates: Vec::new(),
            false_positive_rates: Vec::new(),
            thresholds: Vec::new(),
            auc: 0.0,
            optimal_threshold: 0.0,
            optimal_sensitivity: 0.0,
            optimal_specificity: 0.0,
            youden_index: 0.0,
        }
    }
}

impl CrossValidation {
    fn new(k_folds: usize, random_seed: u64) -> Self {
        Self {
            k_folds,
            random_seed,
        }
    }

    /// Perform k-fold cross-validation for model performance assessment.
    ///
    /// The closure receives (train_data, test_data) and must return a `Vec<f64>` of
    /// predicted probability scores for each test sample. Accuracy is computed via
    /// threshold at 0.5; AUC is computed via the Mann-Whitney U statistic.
    pub fn k_fold_validation<F>(
        &self,
        data: &[(Vec<f64>, bool)],
        mut model_fn: F,
    ) -> Result<CrossValidationResult>
    where
        F: FnMut(&[(Vec<f64>, bool)], &[(Vec<f64>, bool)]) -> Result<Vec<f64>>,
    {
        if data.len() < self.k_folds {
            return Err(SteganoError::InvalidParameter(
                "Not enough data for k-fold validation".to_string(),
            ));
        }

        let fold_size = data.len() / self.k_folds;
        let mut fold_accuracies = Vec::new();
        let mut fold_aucs = Vec::new();

        for fold in 0..self.k_folds {
            let test_start = fold * fold_size;
            let test_end = if fold == self.k_folds - 1 {
                data.len()
            } else {
                (fold + 1) * fold_size
            };

            let test_data = &data[test_start..test_end];
            let train_data: Vec<_> = data
                .iter()
                .enumerate()
                .filter(|(i, _)| *i < test_start || *i >= test_end)
                .map(|(_, item)| item.clone())
                .collect();

            // Model returns predicted probability scores for each test sample
            let scores = model_fn(&train_data, test_data)?;

            // Accuracy: threshold predictions at 0.5
            let accuracy = if scores.is_empty() {
                0.0
            } else {
                scores
                    .iter()
                    .zip(test_data.iter())
                    .filter(|(&s, (_, label))| (s >= 0.5) == *label)
                    .count() as f64
                    / scores.len() as f64
            };
            fold_accuracies.push(accuracy);

            // AUC via Mann-Whitney U statistic (O(n_pos * n_neg), n small in each fold)
            let true_labels: Vec<bool> = test_data.iter().map(|(_, l)| *l).collect();
            fold_aucs.push(Self::mann_whitney_auc(&true_labels, &scores));
        }

        let mean_accuracy = fold_accuracies.iter().sum::<f64>() / fold_accuracies.len() as f64;
        let variance = fold_accuracies
            .iter()
            .map(|x| (x - mean_accuracy).powi(2))
            .sum::<f64>()
            / fold_accuracies.len() as f64;
        let std_accuracy = variance.sqrt();

        // 95% confidence interval
        let t_value = 2.776; // t-value for 95% CI with 4 df (5-fold CV)
        let margin_of_error = t_value * std_accuracy / (fold_accuracies.len() as f64).sqrt();
        let confidence_interval = (
            mean_accuracy - margin_of_error,
            mean_accuracy + margin_of_error,
        );

        let mean_auc = fold_aucs.iter().sum::<f64>() / fold_aucs.len() as f64;

        Ok(CrossValidationResult {
            fold_accuracies,
            mean_accuracy,
            std_accuracy,
            confidence_interval,
            fold_aucs,
            mean_auc,
        })
    }

    /// AUC via the Mann–Whitney U statistic: the fraction of (positive, negative)
    /// score pairs where the positive score is higher, counting ties as ½.
    /// Rank-based, `O(n log n)`. Returns 0.5 when either class is empty.
    fn mann_whitney_auc(labels: &[bool], scores: &[f64]) -> f64 {
        let n = labels.len().min(scores.len());
        let n_pos = labels[..n].iter().filter(|&&l| l).count();
        let n_neg = n - n_pos;
        if n_pos == 0 || n_neg == 0 {
            return 0.5; // undefined; return chance level
        }

        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| scores[a].total_cmp(&scores[b]));

        // Sum of mid-ranks (1-based) of the positive samples.
        let mut rank_sum_pos = 0.0f64;
        let mut i = 0;
        while i < idx.len() {
            let mut j = i;
            while j + 1 < idx.len() && scores[idx[j + 1]] == scores[idx[i]] {
                j += 1;
            }
            let mid_rank = (i + j) as f64 / 2.0 + 1.0;
            for &k in &idx[i..=j] {
                if labels[k] {
                    rank_sum_pos += mid_rank;
                }
            }
            i = j + 1;
        }
        let (np, nn) = (n_pos as f64, n_neg as f64);
        let u = rank_sum_pos - np * (np + 1.0) / 2.0;
        u / (np * nn)
    }
}

// ---------------------------------------------------------------------------
// Distribution functions
// ---------------------------------------------------------------------------

/// Natural log of the gamma function, `ln Γ(x)` (Lanczos approximation, g = 7).
fn ln_gamma(x: f64) -> f64 {
    const COEF: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection formula
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let mut a = COEF[0];
    for (i, &c) in COEF.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Regularized upper incomplete gamma function `Q(a, x) = Γ(a, x) / Γ(a)`.
///
/// Uses the power series for `x < a + 1` and Lentz's continued fraction
/// otherwise (Numerical Recipes §6.2).
pub fn regularized_gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if a <= 0.0 {
        return 0.0;
    }
    const EPS: f64 = 1e-15;
    const TINY: f64 = 1e-300;
    let log_prefactor = -x + a * x.ln() - ln_gamma(a);
    if x < a + 1.0 {
        let mut ap = a;
        let mut del = 1.0 / a;
        let mut sum = del;
        for _ in 0..10_000 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * EPS {
                break;
            }
        }
        (1.0 - sum * log_prefactor.exp()).clamp(0.0, 1.0)
    } else {
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / TINY;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..10_000 {
            let an = -(i as f64) * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < TINY {
                d = TINY;
            }
            c = b + an / c;
            if c.abs() < TINY {
                c = TINY;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < EPS {
                break;
            }
        }
        (log_prefactor.exp() * h).clamp(0.0, 1.0)
    }
}

/// χ² survival function `P(χ²_df ≥ x)` — the one-tailed (upper) p-value.
///
/// Computed as `Q(df/2, x/2)`. Returns 1 for `df == 0`.
pub fn chi_square_survival(x: f64, df: usize) -> f64 {
    if df == 0 || x.is_nan() {
        return 1.0;
    }
    regularized_gamma_q(df as f64 / 2.0, x.max(0.0) / 2.0)
}

/// Upper-tail χ² quantile: the value `c` with `P(χ²_df ≥ c) = alpha`.
///
/// Solved by bisection on [`chi_square_survival`]; e.g. `df = 1, α = 0.05`
/// gives 3.841 and `df = 10, α = 0.05` gives 18.307.
pub fn chi_square_upper_quantile(alpha: f64, df: usize) -> f64 {
    if df == 0 || alpha >= 1.0 {
        return 0.0;
    }
    let alpha = alpha.max(1e-300);
    let mut lo = 0.0;
    let mut hi = (df as f64).max(1.0);
    while chi_square_survival(hi, df) > alpha && hi < 1e12 {
        hi *= 2.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chi_square_survival(mid, df) > alpha {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-12 * hi.max(1.0) {
            break;
        }
    }
    0.5 * (lo + hi)
}

// ---------------------------------------------------------------------------
// LSB detector internals
// ---------------------------------------------------------------------------

/// Minimum `h[2k] + h[2k+1]` for a value pair to enter the PoV statistic
/// (expected count ≥ 5 per cell).
const POV_MIN_PAIR_COUNT: f64 = 10.0;
/// Significance level used by the LSB detectors.
const DETECTOR_ALPHA: f64 = 0.05;

/// Westfeld–Pfitzmann pairs-of-values attack result. See
/// [`AdvancedSteganalysis::pairs_of_values_test`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairsOfValuesResult {
    /// Pearson χ² against the pair-equalised (stego) histogram.
    pub chi_square_statistic: f64,
    /// Degrees of freedom (= `pairs_used`).
    pub degrees_of_freedom: usize,
    /// `P(χ²_df ≥ statistic)`: high ⇒ histogram consistent with full LSB
    /// replacement; ≈ 0 ⇒ histogram has cover-like pair asymmetry.
    pub embedding_probability: f64,
    /// Number of value pairs `(2k, 2k+1)` with enough samples to be tested.
    pub pairs_used: usize,
    /// Stego hypothesis not rejected at α = 0.05.
    pub embedding_detected: bool,
}

/// Sample Pairs Analysis result. See
/// [`AdvancedSteganalysis::sample_pairs_analysis`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplePairsResult {
    /// Estimated LSB-replacement embedding rate, clamped to `[0, 1]`.
    pub estimated_rate: f64,
    /// Unclamped SPA root (continuous detector score).
    pub raw_estimate: f64,
    /// Total adjacent pairs `|P|`.
    pub pairs_analyzed: usize,
    /// `|X|`.
    pub x_pairs: usize,
    /// `|Y|`.
    pub y_pairs: usize,
    /// `|K| = |W ∪ Z|`, pairs with `⌊u/2⌋ = ⌊v/2⌋`.
    pub closed_pairs: usize,
    /// `(|X| − |Y|)² / (|X| + |Y|)`.
    pub balance_chi_square: f64,
    /// `P(χ²₁ ≥ balance_chi_square)`; small ⇒ evidence of embedding.
    pub balance_p_value: f64,
    /// `estimated_rate > threshold` and `balance_p_value < 0.05`.
    pub embedding_detected: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct SpaCounts {
    pairs: usize,
    x: usize,
    y: usize,
    k: usize,
}

impl SpaCounts {
    #[inline]
    fn add(&mut self, u: u8, v: u8) {
        self.pairs += 1;
        let v_even = v & 1 == 0;
        if (v_even && u < v) || (!v_even && u > v) {
            self.x += 1;
        } else if u != v {
            self.y += 1;
        }
        if u >> 1 == v >> 1 {
            self.k += 1;
        }
    }
}

/// Count SPA pair classes over horizontal (and, if `height > 1`, vertical)
/// neighbours. Rows are `width * channels` bytes; a trailing partial row is
/// included for horizontal pairs only.
fn spa_counts(samples: &[u8], width: usize, height: usize, channels: usize) -> SpaCounts {
    let mut c = SpaCounts::default();
    let stride = width * channels;
    for row in samples.chunks(stride) {
        for i in channels..row.len() {
            c.add(row[i - channels], row[i]);
        }
    }
    if height > 1 {
        let full = (samples.len() / stride).min(height);
        for r in 1..full {
            let prev = &samples[(r - 1) * stride..r * stride];
            let cur = &samples[r * stride..(r + 1) * stride];
            for (&u, &v) in prev.iter().zip(cur) {
                c.add(u, v);
            }
        }
    }
    c
}

/// Smaller root of the SPA quadratic; `NaN` if undefined.
fn spa_rate(c: &SpaCounts) -> f64 {
    let a = c.k as f64 / 2.0;
    let b = 2.0 * c.x as f64 - c.pairs as f64;
    let cc = c.y as f64 - c.x as f64;
    if a <= 0.0 {
        // Degenerate (linear) case
        return if b != 0.0 { -cc / b } else { f64::NAN };
    }
    let disc = b * b - 4.0 * a * cc;
    if disc < 0.0 {
        // No real root: the vertex is the closest estimate
        return -b / (2.0 * a);
    }
    let sq = disc.sqrt();
    ((-b - sq) / (2.0 * a)).min((-b + sq) / (2.0 * a))
}

/// Deterministic SplitMix64 PRNG used to simulate embedding in validation.
#[derive(Debug, Clone)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Threshold `t` maximising training accuracy of the rule `score ≥ t ⇒ stego`.
fn fit_threshold(train: &[(Vec<f64>, bool)]) -> f64 {
    let mut pts: Vec<(f64, bool)> = train.iter().map(|(f, l)| (f[0], *l)).collect();
    if pts.is_empty() {
        return 0.0;
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total_pos = pts.iter().filter(|p| p.1).count();
    // Threshold below everything: all predicted positive.
    let mut best_correct = total_pos;
    let mut best_t = pts[0].0 - 1.0;
    let mut neg_below = 0usize;
    let mut pos_below = 0usize;
    for i in 0..pts.len() {
        if pts[i].1 {
            pos_below += 1;
        } else {
            neg_below += 1;
        }
        // Only split between distinct scores
        if i + 1 < pts.len() && pts[i + 1].0 == pts[i].0 {
            continue;
        }
        let correct = neg_below + (total_pos - pos_below);
        if correct > best_correct {
            best_correct = correct;
            best_t = if i + 1 < pts.len() {
                0.5 * (pts[i].0 + pts[i + 1].0)
            } else {
                pts[i].0 + 1.0
            };
        }
    }
    best_t
}

/// Standard deviation of training scores (floored to keep the score map finite).
fn score_spread(train: &[(Vec<f64>, bool)]) -> f64 {
    if train.is_empty() {
        return 1.0;
    }
    let n = train.len() as f64;
    let mean = train.iter().map(|(f, _)| f[0]).sum::<f64>() / n;
    let var = train
        .iter()
        .map(|(f, _)| (f[0] - mean).powi(2))
        .sum::<f64>()
        / n;
    var.sqrt().max(1e-9)
}

/// Simulate LSB replacement: each sample is selected with probability `rate`
/// and its LSB replaced by a uniformly random bit (so ≈ `rate / 2` of samples
/// actually change). This is the per-sample operation the LSB handlers apply
/// with an encrypted (pseudo-random) payload.
fn simulate_lsb_replacement(cover: &[u8], rate: f64, rng: &mut SplitMix64) -> Vec<u8> {
    cover
        .iter()
        .map(|&b| {
            if rng.next_f64() < rate {
                (b & !1) | (rng.next_u64() & 1) as u8
            } else {
                b
            }
        })
        .collect()
}

/// Domain-specific validation for scientific data
#[derive(Debug)]
#[allow(dead_code)]
pub struct DomainSpecificValidation {
    genomics_validator: GenomicsValidator,
    medical_imaging_validator: MedicalImagingValidator,
    audio_validator: AudioValidator,
}

/// Genomics data validation
#[derive(Debug)]
pub struct GenomicsValidator;

/// Medical imaging validation
#[derive(Debug)]
pub struct MedicalImagingValidator;

/// Audio data validation
#[derive(Debug)]
pub struct AudioValidator;

/// Test dataset management
#[derive(Debug)]
pub struct TestDatasetManager {
    datasets: HashMap<String, TestDataset>,
}

/// Standardized test dataset
#[derive(Debug, Clone)]
pub struct TestDataset {
    pub name: String,
    pub description: String,
    pub data: Vec<u8>,
    pub format: DataFormat,
    pub ground_truth: Option<GroundTruth>,
}

/// Ground truth information for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundTruth {
    pub has_hidden_data: bool,
    pub embedding_method: Option<String>,
    pub payload_size: Option<usize>,
    pub embedding_parameters: Option<EmbedParams>,
}

impl StatisticalValidationSuite {
    /// Create a new comprehensive statistical validation suite
    pub fn new() -> Self {
        Self {
            classical_tests: ClassicalStatisticalTests::new(0.95),
            steganalysis: AdvancedSteganalysis::new(0.1),
            performance_analysis: PerformanceAnalysis::new(),
            domain_validation: DomainSpecificValidation::new(),
            test_datasets: TestDatasetManager::new(),
        }
    }

    /// Run complete validation suite on steganographic data
    pub fn comprehensive_validation(
        &self,
        original: &[u8],
        stego: &[u8],
        format: &DataFormat,
    ) -> Result<ValidationReport> {
        let mut report = ValidationReport::new();

        // Classical statistical tests
        report.classical_results = self.run_classical_tests(original, stego)?;

        // Advanced steganalysis
        report.steganalysis_results = self.run_steganalysis(stego, format)?;

        // Performance analysis
        report.performance_results = self.run_performance_analysis(original, stego)?;

        // Domain-specific validation
        report.domain_results = self.run_domain_validation(original, stego, format)?;

        Ok(report)
    }

    fn run_classical_tests(&self, original: &[u8], stego: &[u8]) -> Result<ClassicalTestResults> {
        let original_f64: Vec<f64> = original.iter().map(|&x| x as f64).collect();
        let stego_f64: Vec<f64> = stego.iter().map(|&x| x as f64).collect();

        // Create histograms for chi-square test to ensure positive frequencies
        let (observed_hist, expected_hist) = self.create_histograms(&stego_f64, &original_f64);

        // Run five independent tests in parallel using rayon::join chains
        let ct = &self.classical_tests;
        let ((ks_result, chi_result), (runs_result, (ad_result, autocorr_result))) = rayon::join(
            || {
                rayon::join(
                    || ct.kolmogorov_smirnov_test(&original_f64, &stego_f64),
                    || ct.chi_square_test(&observed_hist, &expected_hist),
                )
            },
            || {
                rayon::join(
                    || {
                        let binary_seq: Vec<bool> = stego.iter().map(|&x| x & 1 == 1).collect();
                        ct.runs_test(&binary_seq)
                    },
                    || {
                        rayon::join(
                            || ct.anderson_darling_test(&stego_f64),
                            || ct.autocorrelation_analysis(&stego_f64, 20),
                        )
                    },
                )
            },
        );

        Ok(ClassicalTestResults {
            ks_test: ks_result?,
            chi_square_test: chi_result?,
            runs_test: runs_result?,
            anderson_darling: ad_result?,
            autocorrelation: autocorr_result?,
        })
    }

    fn create_histograms(
        &self,
        observed_data: &[f64],
        expected_data: &[f64],
    ) -> (Vec<f64>, Vec<f64>) {
        const BINS: usize = 10;
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

    fn run_steganalysis(&self, stego: &[u8], format: &DataFormat) -> Result<SteganalysisResults> {
        let mut results = SteganalysisResults::default();

        match format {
            DataFormat::PNG | DataFormat::TIFF => {
                // Dimensions are unknown here, so the samples are analysed as a
                // single row (1-D adjacent pairs). Callers that know the image
                // geometry should call `pixel_pair_analysis` directly.
                results.pixel_pair_analysis = Some(self.steganalysis.pixel_pair_analysis(
                    stego,
                    stego.len(),
                    1,
                )?);
                results.pairs_of_values = Some(self.steganalysis.pairs_of_values_test(stego)?);
                results.wavelet_analysis =
                    Some(self.steganalysis.wavelet_structural_analysis(stego)?);
            }
            DataFormat::WAV => {
                let audio_data: Vec<f32> = stego
                    .chunks(4)
                    .map(|chunk| {
                        let mut bytes = [0u8; 4];
                        for (i, &b) in chunk.iter().enumerate() {
                            if i < 4 {
                                bytes[i] = b;
                            }
                        }
                        f32::from_le_bytes(bytes)
                    })
                    .collect();
                // The sampling rate is not known at this layer; 44.1 kHz is assumed.
                // `SteganoEmbedder` validation uses `carrier.sampling_rate` instead.
                results.audio_analysis =
                    Some(self.steganalysis.audio_steganalysis(&audio_data, 44100.0)?);
            }
            _ => {
                results.wavelet_analysis =
                    Some(self.steganalysis.wavelet_structural_analysis(stego)?);
            }
        }

        Ok(results)
    }

    /// Detector performance on cover/stego sample sets derived from the carrier.
    ///
    /// The detector is Sample Pairs Analysis (its unclamped rate estimate is the
    /// score; higher ⇒ more likely stego). Samples are built as follows:
    ///
    /// 1. The *observed* embedding rate is `min(1, 2 · fraction of samples that
    ///    differ between original and stego)` — LSB replacement with random bits
    ///    changes about half of the samples it touches.
    /// 2. The original carrier is cut into equal segments (32–8192 bytes,
    ///    about 40 segments). Each segment yields one **cover** sample (as-is)
    ///    and one **stego** sample (the same segment after simulated LSB
    ///    replacement of random bits at the observed rate, deterministic seed).
    ///    Segments are analysed as 1-D signals (adjacent-byte pairs).
    ///
    /// `roc_analysis` is computed over all samples (SPA is unsupervised, so no
    /// training leakage). `cross_validation` is honest k-fold CV: for each fold
    /// the decision threshold is fit on the training folds (maximum training
    /// accuracy) and accuracy/AUC are evaluated on the held-out fold only.
    ///
    /// An AUC near 0.5 means the embedding at this rate is statistically
    /// invisible to SPA on this carrier; near 1.0 means it is reliably detected.
    pub fn run_performance_analysis(
        &self,
        original: &[u8],
        stego: &[u8],
    ) -> Result<PerformanceResults> {
        let n = original.len().min(stego.len());
        if n < 20 {
            return Ok(PerformanceResults::default());
        }

        let changed = original[..n]
            .iter()
            .zip(&stego[..n])
            .filter(|(o, s)| o != s)
            .count();
        let rate = (2.0 * changed as f64 / n as f64).min(1.0);

        let segment_len = (n / 40).clamp(32, 8192);
        let segments = n / segment_len;

        let mut rng = SplitMix64(0x5354_4547_414E_4F00 ^ n as u64);
        let mut true_labels = Vec::with_capacity(segments * 2);
        let mut scores = Vec::with_capacity(segments * 2);
        let spa_score = |seg: &[u8]| -> f64 {
            let c = spa_counts(seg, seg.len(), 1, 1);
            let r = spa_rate(&c);
            if r.is_finite() {
                r
            } else {
                0.0
            }
        };
        for seg in original[..segments * segment_len].chunks_exact(segment_len) {
            true_labels.push(false);
            scores.push(spa_score(seg));
            let stego_seg = simulate_lsb_replacement(seg, rate, &mut rng);
            true_labels.push(true);
            scores.push(spa_score(&stego_seg));
        }

        let roc = self
            .performance_analysis
            .roc_analysis(&true_labels, &scores)
            .ok();

        // Samples are interleaved (cover, stego, cover, …) so every contiguous
        // fold is class-balanced.
        let cv_data: Vec<(Vec<f64>, bool)> = scores
            .iter()
            .zip(&true_labels)
            .map(|(&s, &l)| (vec![s], l))
            .collect();
        let cv_result = if cv_data.len() >= 10 {
            self.performance_analysis
                .cross_validation
                .k_fold_validation(&cv_data, |train, test| {
                    let threshold = fit_threshold(train);
                    let spread = score_spread(train);
                    // Monotone map with 0.5 exactly at the fitted threshold, so the
                    // CV accuracy rule (score ≥ 0.5) equals "raw score ≥ threshold"
                    // and the AUC is that of the raw detector score.
                    Ok(test
                        .iter()
                        .map(|(f, _)| 0.5 + 0.5 * ((f[0] - threshold) / spread).tanh())
                        .collect())
                })
                .ok()
        } else {
            None
        };

        Ok(PerformanceResults {
            roc_analysis: roc,
            cross_validation: cv_result,
            detector: "sample_pairs_analysis".to_string(),
            simulated_embedding_rate: rate,
            samples: true_labels.len(),
        })
    }

    fn run_domain_validation(
        &self,
        _original: &[u8],
        stego: &[u8],
        format: &DataFormat,
    ) -> Result<DomainResults> {
        let mut results = DomainResults::default();

        if format.is_genomics() {
            // Verify the stego file still looks like valid genomic text (SAM/FASTQ/VCF)
            // or binary (BAM).  For text-based formats check printable ASCII; for BAM
            // accept any bytes (it is binary).
            let genomics_ok = match format {
                DataFormat::BAM => {
                    // BAM: binary, only check for the mandatory BAM magic "BAM\1"
                    stego.len() >= 4 && stego[..4] == [0x42, 0x41, 0x4D, 0x01]
                }
                _ => {
                    // SAM/FASTQ/FASTA/VCF: text-based; verify >95 % of bytes are printable ASCII
                    let printable = stego
                        .iter()
                        .filter(|&&b| {
                            b == b'\n' || b == b'\r' || b == b'\t' || (32..127).contains(&b)
                        })
                        .count();
                    printable * 20 >= stego.len() * 19 // ≥95 % printable
                }
            };
            results.genomics_integrity = Some(genomics_ok);
        } else if format.is_imagery() {
            // Medical/imaging formats: verify known magic / structure markers are intact.
            let medical_ok = match format {
                DataFormat::DICOM => stego.len() >= 132 && &stego[128..132] == b"DICM",
                DataFormat::TIFF => {
                    stego.len() >= 4
                        && ((&stego[..2] == b"II" && stego[2] == 42)
                            || (&stego[..2] == b"MM" && stego[2] == 0 && stego[3] == 42))
                }
                DataFormat::PNG => {
                    stego.len() >= 8
                        && stego[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
                }
                _ => true, // unknown imaging sub-format — assume OK
            };
            results.medical_compliance = Some(medical_ok);
        } else if matches!(format, DataFormat::WAV | DataFormat::BinarySignal) {
            // Audio/signal quality: compute normalised RMS of the signal bytes (skipping a
            // 44-byte WAV header for WAV files).
            let audio_start = if *format == DataFormat::WAV { 44 } else { 0 };
            let audio_data = if audio_start < stego.len() {
                &stego[audio_start..]
            } else {
                stego
            };
            if !audio_data.is_empty() {
                let mean =
                    audio_data.iter().map(|&b| b as f64).sum::<f64>() / audio_data.len() as f64;
                let rms = (audio_data
                    .iter()
                    .map(|&b| (b as f64 - mean).powi(2))
                    .sum::<f64>()
                    / audio_data.len() as f64)
                    .sqrt();
                // Normalise to [0, 1] against maximum possible RMS of 127.5
                let quality = (rms / 127.5).min(1.0);
                results.audio_quality = Some(quality);
            }
        }

        Ok(results)
    }

    /// Access to classical statistical tests
    pub fn classical_tests(&self) -> &ClassicalStatisticalTests {
        &self.classical_tests
    }

    /// Access to advanced steganalysis
    pub fn advanced_steganalysis(&self) -> &AdvancedSteganalysis {
        &self.steganalysis
    }

    /// Access to performance analysis
    pub fn performance_analysis(&self) -> &PerformanceAnalysis {
        &self.performance_analysis
    }

    /// Access to domain-specific validation
    pub fn domain_specific(&self) -> &DomainSpecificValidation {
        &self.domain_validation
    }
}

/// Lightweight summary of a validation run
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub tests_run: usize,
    pub passed: usize,
    pub overall_score: f64,
    pub status: String,
}

/// Comprehensive validation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub classical_results: ClassicalTestResults,
    pub steganalysis_results: SteganalysisResults,
    pub performance_results: PerformanceResults,
    pub domain_results: DomainResults,
    pub overall_score: f64,
    pub recommendations: Vec<String>,
    pub test_results: std::collections::HashMap<String, serde_json::Value>,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassicalTestResults {
    pub ks_test: KSTestResult,
    pub chi_square_test: ChiSquareResult,
    pub runs_test: RunsTestResult,
    pub anderson_darling: AndersonDarlingResult,
    pub autocorrelation: AutocorrelationResult,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SteganalysisResults {
    pub pixel_pair_analysis: Option<PixelPairAnalysisResult>,
    /// Westfeld–Pfitzmann pairs-of-values attack (image formats).
    #[serde(default)]
    pub pairs_of_values: Option<PairsOfValuesResult>,
    pub wavelet_analysis: Option<WaveletAnalysisResult>,
    pub audio_analysis: Option<AudioSteganalysisResult>,
}

/// Detector performance over cover/stego sample sets. See
/// [`StatisticalValidationSuite::run_performance_analysis`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceResults {
    /// ROC of the detector score over all cover/stego samples.
    pub roc_analysis: Option<ROCAnalysis>,
    /// k-fold CV with the decision threshold fit on the training folds only.
    pub cross_validation: Option<CrossValidationResult>,
    /// Detector whose score was evaluated.
    #[serde(default)]
    pub detector: String,
    /// Embedding rate used for the simulated stego samples.
    #[serde(default)]
    pub simulated_embedding_rate: f64,
    /// Total number of (cover + stego) samples.
    #[serde(default)]
    pub samples: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DomainResults {
    pub genomics_integrity: Option<bool>,
    pub medical_compliance: Option<bool>,
    pub audio_quality: Option<f64>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            classical_results: ClassicalTestResults {
                ks_test: KSTestResult {
                    test_statistic: 0.0,
                    p_value: 1.0,
                    critical_value: 0.0,
                    is_significant: false,
                    max_difference_location: 0,
                },
                chi_square_test: ChiSquareResult {
                    test_statistic: 0.0,
                    degrees_of_freedom: 0,
                    p_value: 1.0,
                    critical_value: 0.0,
                    is_significant: false,
                    effect_size: 0.0,
                },
                runs_test: RunsTestResult {
                    observed_runs: 0,
                    expected_runs: 0.0,
                    variance: 0.0,
                    z_score: 0.0,
                    p_value: 1.0,
                    is_random: true,
                },
                anderson_darling: AndersonDarlingResult {
                    test_statistic: 0.0,
                    p_value: 1.0,
                    critical_values: vec![],
                    significance_levels: vec![],
                    is_normal: true,
                },
                autocorrelation: AutocorrelationResult {
                    correlations: vec![],
                    significant_lags: vec![],
                    ljung_box_statistic: 0.0,
                    ljung_box_p_value: 1.0,
                    has_correlation: false,
                },
            },
            steganalysis_results: SteganalysisResults::default(),
            performance_results: PerformanceResults::default(),
            domain_results: DomainResults::default(),
            overall_score: 0.0,
            recommendations: vec![],
            test_results: std::collections::HashMap::new(),
            is_complete: false,
        }
    }

    pub fn add_test_result(&mut self, name: String, value: serde_json::Value) {
        self.test_results.insert(name, value);
    }

    pub fn finalize(&mut self) {
        self.is_complete = true;
    }

    pub fn overall_score(&self) -> u8 {
        self.overall_score as u8
    }

    pub fn test_results(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        &self.test_results
    }

    /// Return a compact summary of the validation results.
    pub fn summary(&self) -> ValidationSummary {
        let cr = &self.classical_results;
        let tests_run = 5;
        let mut passed = 0usize;

        if !cr.ks_test.is_significant {
            passed += 1;
        }
        if !cr.chi_square_test.is_significant {
            passed += 1;
        }
        if cr.runs_test.is_random {
            passed += 1;
        }
        if cr.anderson_darling.is_normal {
            passed += 1;
        }
        if !cr.autocorrelation.has_correlation {
            passed += 1;
        }

        let status = if passed == tests_run {
            "PASS".to_string()
        } else if passed * 2 >= tests_run {
            "WARN".to_string()
        } else {
            "FAIL".to_string()
        };

        ValidationSummary {
            tests_run,
            passed,
            overall_score: self.overall_score,
            status,
        }
    }

    /// Serialize the report to a pretty-printed JSON string.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary = self.summary();
        let cr = &self.classical_results;

        writeln!(f, "╔══════════════════════════════════════╗")?;
        writeln!(f, "║      Stegano Validation Dashboard    ║")?;
        writeln!(f, "╚══════════════════════════════════════╝")?;
        writeln!(
            f,
            "Tests completed: {}  |  Status: {}",
            summary.tests_run, summary.status
        )?;
        writeln!(f)?;
        writeln!(f, "Classical Tests")?;

        let ok_fail = |pass: bool| if pass { "[ OK ]" } else { "[FAIL]" };

        writeln!(
            f,
            "  chi_square  χ²={:.3}  p={:.3}  {}",
            cr.chi_square_test.test_statistic,
            cr.chi_square_test.p_value,
            ok_fail(!cr.chi_square_test.is_significant)
        )?;
        writeln!(
            f,
            "  ks_test     D={:.3}   p={:.3}  {}",
            cr.ks_test.test_statistic,
            cr.ks_test.p_value,
            ok_fail(!cr.ks_test.is_significant)
        )?;
        writeln!(
            f,
            "  runs_test   z={:.3}   p={:.3}  {}",
            cr.runs_test.z_score,
            cr.runs_test.p_value,
            ok_fail(cr.runs_test.is_random)
        )?;
        writeln!(
            f,
            "  anderson_d  A²={:.3}  p={:.3}  {}",
            cr.anderson_darling.test_statistic,
            cr.anderson_darling.p_value,
            ok_fail(cr.anderson_darling.is_normal)
        )?;
        writeln!(
            f,
            "  autocorr    LB={:.3}  p={:.3}  {}",
            cr.autocorrelation.ljung_box_statistic,
            cr.autocorrelation.ljung_box_p_value,
            ok_fail(!cr.autocorrelation.has_correlation)
        )?;

        if !self.recommendations.is_empty() {
            writeln!(f)?;
            writeln!(f, "Recommendations")?;
            for rec in &self.recommendations {
                writeln!(f, "  \u{2022} {rec}")?;
            }
        }

        Ok(())
    }
}

impl DomainSpecificValidation {
    fn new() -> Self {
        Self {
            genomics_validator: GenomicsValidator,
            medical_imaging_validator: MedicalImagingValidator,
            audio_validator: AudioValidator,
        }
    }
}

impl TestDatasetManager {
    pub fn new() -> Self {
        Self {
            datasets: HashMap::new(),
        }
    }

    /// Load standardized test datasets
    pub fn load_standard_datasets(&mut self) -> Result<()> {
        // Create standard test datasets for validation
        self.create_synthetic_datasets()?;
        Ok(())
    }

    fn create_synthetic_datasets(&mut self) -> Result<()> {
        // Create synthetic PNG test data
        let png_data = self.create_synthetic_png(256, 256)?;
        self.datasets.insert(
            "synthetic_png".to_string(),
            TestDataset {
                name: "Synthetic PNG".to_string(),
                description: "256x256 synthetic image for testing".to_string(),
                data: png_data,
                format: DataFormat::PNG,
                ground_truth: Some(GroundTruth {
                    has_hidden_data: false,
                    embedding_method: None,
                    payload_size: None,
                    embedding_parameters: None,
                }),
            },
        );

        // Create synthetic audio test data
        let audio_data = self.create_synthetic_audio(44100, 1.0)?; // 1 second at 44.1kHz
        self.datasets.insert(
            "synthetic_audio".to_string(),
            TestDataset {
                name: "Synthetic Audio".to_string(),
                description: "1 second sine wave at 440Hz".to_string(),
                data: audio_data,
                format: DataFormat::WAV,
                ground_truth: Some(GroundTruth {
                    has_hidden_data: false,
                    embedding_method: None,
                    payload_size: None,
                    embedding_parameters: None,
                }),
            },
        );

        Ok(())
    }

    pub fn create_synthetic_png(&self, width: usize, height: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(width * height * 3); // RGB

        for y in 0..height {
            for x in 0..width {
                // Create a gradient pattern
                let r = (255.0 * x as f64 / width as f64) as u8;
                let g = (255.0 * y as f64 / height as f64) as u8;
                let b = (255.0 * ((x + y) as f64 / (width + height) as f64)) as u8;

                data.push(r);
                data.push(g);
                data.push(b);
            }
        }

        Ok(data)
    }

    pub fn create_synthetic_audio(&self, sample_rate: usize, duration: f32) -> Result<Vec<u8>> {
        let num_samples = (sample_rate as f32 * duration) as usize;
        let mut audio_data = Vec::with_capacity(num_samples * 4); // 32-bit float

        for i in 0..num_samples {
            // Generate 440Hz sine wave
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;

            // Convert to bytes
            let bytes = sample.to_le_bytes();
            audio_data.extend_from_slice(&bytes);
        }

        Ok(audio_data)
    }

    /// Get dataset by name
    pub fn get_dataset(&self, name: &str) -> Option<&TestDataset> {
        self.datasets.get(name)
    }

    /// List available datasets
    pub fn list_datasets(&self) -> Vec<String> {
        self.datasets.keys().cloned().collect()
    }
}

impl Default for TestDatasetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for StatisticalValidationSuite {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chi_square_test() {
        let validation = StatisticalValidationSuite::new();
        let observed = vec![10.0, 15.0, 20.0, 25.0];
        let expected = vec![12.0, 16.0, 18.0, 24.0];

        let result = validation
            .classical_tests
            .chi_square_test(&observed, &expected)
            .unwrap();
        assert!(result.test_statistic > 0.0);
        assert!(result.degrees_of_freedom == 3);
        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    #[test]
    fn test_ks_test() {
        let validation = StatisticalValidationSuite::new();
        let sample1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let sample2 = vec![1.1, 2.1, 3.1, 4.1, 5.1];

        let result = validation
            .classical_tests
            .kolmogorov_smirnov_test(&sample1, &sample2)
            .unwrap();
        assert!(result.test_statistic >= 0.0);
        assert!(result.p_value >= 0.0, "p_value: {}", result.p_value);
        assert!(result.p_value <= 1.0, "p_value: {}", result.p_value);
    }

    #[test]
    fn test_runs_test() {
        let validation = StatisticalValidationSuite::new();
        let sequence = vec![true, false, true, false, true, false, true, false];

        let result = validation.classical_tests.runs_test(&sequence).unwrap();
        assert!(result.observed_runs > 0);
        assert!(result.expected_runs > 0.0);
    }

    #[test]
    fn test_pixel_pair_analysis() {
        let validation = StatisticalValidationSuite::new();
        let image_data = vec![100u8; 1024]; // Simple test image

        let result = validation
            .steganalysis
            .pixel_pair_analysis(&image_data, 32, 32)
            .unwrap();
        assert!(result.embedding_probability >= 0.0 && result.embedding_probability <= 1.0);
    }

    #[test]
    fn test_wavelet_analysis() {
        let validation = StatisticalValidationSuite::new();
        let data = vec![128u8; 1024]; // Simple test data

        let result = validation
            .steganalysis
            .wavelet_structural_analysis(&data)
            .unwrap();
        assert!(!result.energy_distribution.is_empty());
        assert!(result.anomaly_score >= 0.0);
    }

    #[test]
    fn test_audio_steganalysis() {
        let validation = StatisticalValidationSuite::new();
        // Create enough audio data (need at least sample_rate length)
        let audio_data: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin() * 0.5)
            .collect();

        let result = validation
            .steganalysis
            .audio_steganalysis(&audio_data, 44100.0)
            .unwrap();
        assert!(
            result.embedding_probability >= 0.0,
            "embedding_probability: {}",
            result.embedding_probability
        );
        assert!(
            result.embedding_probability <= 1.0,
            "embedding_probability: {}",
            result.embedding_probability
        );
    }

    #[test]
    fn test_roc_analysis() {
        let validation = StatisticalValidationSuite::new();
        let true_labels = vec![true, false, true, false, true];
        let scores = vec![0.8, 0.3, 0.9, 0.2, 0.7];

        let result = validation
            .performance_analysis
            .roc_analysis(&true_labels, &scores)
            .unwrap();
        assert!(result.auc >= 0.0 && result.auc <= 1.0);
        assert!(result.optimal_threshold >= 0.0);
    }

    #[test]
    fn test_test_dataset_creation() {
        let mut manager = TestDatasetManager::new();
        manager.load_standard_datasets().unwrap();

        let datasets = manager.list_datasets();
        assert!(datasets.contains(&"synthetic_png".to_string()));
        assert!(datasets.contains(&"synthetic_audio".to_string()));

        let png_dataset = manager.get_dataset("synthetic_png").unwrap();
        assert_eq!(png_dataset.format, DataFormat::PNG);
        assert!(!png_dataset.data.is_empty());
    }

    #[test]
    fn test_comprehensive_validation() {
        let validation = StatisticalValidationSuite::new();
        // Create data with mixed bit patterns to ensure both true and false values
        let original: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let stego: Vec<u8> = (0..1024).map(|i| ((i + 1) % 256) as u8).collect();

        let result = validation
            .comprehensive_validation(&original, &stego, &DataFormat::BINARY)
            .unwrap();
        assert!(result.classical_results.ks_test.p_value >= 0.0);
        assert!(result.overall_score >= 0.0);
    }

    #[test]
    fn test_chi_square_p_value_is_one_tailed() {
        let ct = ClassicalStatisticalTests::new(0.95);
        // A large chi-square should give a small p-value
        let p_large = ct.chi_square_p_value(20.0, 5);
        // A small chi-square should give a large p-value
        let p_small = ct.chi_square_p_value(1.0, 5);
        assert!(
            p_large < p_small,
            "large χ² must give smaller p than small χ²"
        );
        // df=1: well-known value — χ²=3.841 corresponds to p≈0.05
        let p_df1 = ct.chi_square_p_value(3.841, 1);
        assert!(
            (p_df1 - 0.05).abs() < 0.01,
            "χ²₁=3.841 should give p≈0.05, got {}",
            p_df1
        );
        // p-values must be in [0, 1]
        for &chi2 in &[0.0, 1.0, 5.0, 10.0, 50.0] {
            let p = ct.chi_square_p_value(chi2, 4);
            assert!(
                (0.0..=1.0).contains(&p),
                "p out of range for χ²={}: {}",
                chi2,
                p
            );
        }
    }

    #[test]
    fn test_simple_fft_produces_magnitudes() {
        let steg = AdvancedSteganalysis::new(0.1);
        // DC signal: all ones — energy should be concentrated at bin 0
        let dc: Vec<f32> = vec![1.0; 64];
        let mags = steg.simple_fft(&dc).unwrap();
        assert_eq!(mags.len(), 64);
        assert!(mags[0] > 60.0, "DC bin should dominate for constant signal");
        // All-zeros should give all-zero magnitudes
        let zeros = steg.simple_fft(&vec![0.0f32; 64]).unwrap();
        assert!(zeros.iter().all(|&m| m.abs() < 1e-5));
        // Empty input
        let empty = steg.simple_fft(&[]).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_k_fold_auc_is_meaningful() {
        let cv = CrossValidation::new(5, 42);
        // Interleaved positives (score=0.75) and negatives (score=0.25) so every
        // fold has representative samples and the model is a perfect separator.
        let data: Vec<(Vec<f64>, bool)> = (0..100)
            .map(|i| {
                if i % 2 == 0 {
                    (vec![0.75_f64], true)
                } else {
                    (vec![0.25_f64], false)
                }
            })
            .collect();

        let model_fn = |_train: &[(Vec<f64>, bool)],
                        test: &[(Vec<f64>, bool)]|
         -> crate::core::Result<Vec<f64>> {
            Ok(test.iter().map(|(f, _)| f[0]).collect())
        };

        let result = cv.k_fold_validation(&data, model_fn).unwrap();
        // AUC should be close to 1.0 for a near-perfect separator
        assert!(
            result.mean_auc > 0.9,
            "mean AUC should be > 0.9 for separable data, got {}",
            result.mean_auc
        );
        // All fold AUCs should be in [0, 1]
        for &auc in &result.fold_aucs {
            assert!((0.0..=1.0).contains(&auc), "AUC out of range: {}", auc);
        }
    }

    #[test]
    fn test_k_fold_auc_chance_for_random() {
        let cv = CrossValidation::new(5, 42);
        // Random scores uncorrelated with labels — AUC ≈ 0.5
        let data: Vec<(Vec<f64>, bool)> = (0..100)
            .map(|i| (vec![(i % 3) as f64 / 3.0], i % 2 == 0))
            .collect();

        let model_fn =
            |_: &[(Vec<f64>, bool)], test: &[(Vec<f64>, bool)]| -> crate::core::Result<Vec<f64>> {
                // Return constant score — no discrimination
                Ok(vec![0.5; test.len()])
            };

        let result = cv.k_fold_validation(&data, model_fn).unwrap();
        // AUC should be ~0.5 for a non-discriminating model
        assert!(
            (result.mean_auc - 0.5).abs() < 0.2,
            "mean AUC should be near 0.5 for non-discriminating model, got {}",
            result.mean_auc
        );
    }

    #[test]
    fn test_run_performance_analysis() {
        let suite = StatisticalValidationSuite::new();
        // Use enough data for meaningful analysis (> 20 bytes)
        let original: Vec<u8> = (0u8..200).collect();
        let stego: Vec<u8> = original.iter().map(|&b| b ^ 1).collect(); // flip LSB

        let results = suite
            .run_performance_analysis(&original, &stego)
            .expect("performance analysis should not fail");

        // With 200 bytes we expect both ROC and CV to be populated
        assert!(
            results.roc_analysis.is_some(),
            "ROC analysis should be populated for 200-byte input"
        );
        let roc = results.roc_analysis.unwrap();
        assert!(roc.auc >= 0.0 && roc.auc <= 1.0, "AUC must be in [0,1]");

        assert!(
            results.cross_validation.is_some(),
            "Cross-validation should be populated for 200-byte input"
        );
        let cv = results.cross_validation.unwrap();
        assert!(
            cv.mean_accuracy >= 0.0 && cv.mean_accuracy <= 1.0,
            "Mean accuracy must be in [0,1]"
        );
    }

    #[test]
    fn test_run_performance_analysis_too_small() {
        let suite = StatisticalValidationSuite::new();
        let tiny = vec![0u8; 5]; // fewer than 20 bytes
        let results = suite
            .run_performance_analysis(&tiny, &tiny)
            .expect("should return empty PerformanceResults for tiny input");
        assert!(results.roc_analysis.is_none());
        assert!(results.cross_validation.is_none());
    }

    #[test]
    fn test_run_domain_validation_genomics() {
        let suite = StatisticalValidationSuite::new();
        // A SAM-like text payload: all printable ASCII
        let original =
            b"@HD\tVN:1.0\nread1\t0\tchr1\t100\t60\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\n";
        let stego = original; // unchanged
        let results = suite
            .run_domain_validation(original, stego, &DataFormat::SAM)
            .expect("domain validation should not fail");
        assert_eq!(results.genomics_integrity, Some(true));
        assert!(results.medical_compliance.is_none());
        assert!(results.audio_quality.is_none());
    }

    #[test]
    fn test_run_domain_validation_dicom() {
        let suite = StatisticalValidationSuite::new();
        // Minimal DICOM: 128 zero bytes + "DICM"
        let mut stego = vec![0u8; 128];
        stego.extend_from_slice(b"DICM");
        let results = suite
            .run_domain_validation(&stego, &stego, &DataFormat::DICOM)
            .expect("DICOM domain validation should not fail");
        assert_eq!(results.medical_compliance, Some(true));
        assert!(results.genomics_integrity.is_none());
    }

    #[test]
    fn test_run_domain_validation_wav() {
        let suite = StatisticalValidationSuite::new();
        // Minimal WAV-like data: 44-byte header + 100 bytes of audio signal
        let mut stego = vec![0u8; 44]; // fake header
        stego.extend_from_slice(&[128u8; 100]); // silent audio
        let results = suite
            .run_domain_validation(&stego, &stego, &DataFormat::WAV)
            .expect("WAV domain validation should not fail");
        assert!(results.audio_quality.is_some());
        let q = results.audio_quality.unwrap();
        assert!((0.0..=1.0).contains(&q), "audio quality must be in [0,1]");
    }

    // -----------------------------------------------------------------------
    // LSB detector acceptance tests (issue #41)
    // -----------------------------------------------------------------------

    /// Smooth, natural-ish greyscale image: low-frequency gradients plus mild
    /// Gaussian-like sensor noise (σ ≈ 1.4), 8-bit, row-major.
    fn natural_image(width: usize, height: usize, seed: u64) -> Vec<u8> {
        let mut rng = SplitMix64(seed);
        let mut img = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let (xf, yf) = (x as f64, y as f64);
                let base = 110.0
                    + 0.35 * xf
                    + 0.2 * yf
                    + 40.0 * (xf / 19.0).sin() * (yf / 27.0).cos()
                    + 15.0 * ((xf + yf) / 41.0).sin();
                // Sum of 4 uniforms - 2 ~ N(0, 1/3); scale to σ ≈ 1.4
                let noise: f64 = (0..4).map(|_| rng.next_f64()).sum::<f64>() - 2.0;
                img.push((base + 2.4 * noise).round().clamp(0.0, 255.0) as u8);
            }
        }
        img
    }

    /// The same scene after a contrast stretch (×1.4), as done by most camera
    /// and editing pipelines. Stretching leaves a "combed" histogram whose
    /// pairs of values are strongly unbalanced — the cover property PoV uses.
    fn stretched_image(width: usize, height: usize, seed: u64) -> Vec<u8> {
        natural_image(width, height, seed)
            .into_iter()
            .map(|v| ((v as f64 - 60.0) * 1.4).round().clamp(0.0, 255.0) as u8)
            .collect()
    }

    /// Sequential LSB replacement of `fraction` of the samples with random bits.
    fn sequential_lsb_embed(cover: &[u8], fraction: f64, seed: u64) -> Vec<u8> {
        let mut rng = SplitMix64(seed);
        let n = (cover.len() as f64 * fraction).round() as usize;
        cover
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if i < n {
                    (b & !1) | (rng.next_u64() & 1) as u8
                } else {
                    b
                }
            })
            .collect()
    }

    #[test]
    fn test_chi_square_survival_matches_tables() {
        // Reference values from standard χ² tables.
        let cases = [
            (3.841_459, 1, 0.05),
            (6.634_897, 1, 0.01),
            (5.991_465, 2, 0.05),
            (11.070_498, 5, 0.05),
            (18.307_038, 10, 0.05),
            (124.342_113, 100, 0.05),
        ];
        for &(x, df, p) in &cases {
            let got = chi_square_survival(x, df);
            assert!((got - p).abs() < 1e-6, "sf({x}, {df}) = {got}, want {p}");
            let q = chi_square_upper_quantile(p, df);
            assert!((q - x).abs() < 1e-4, "quantile({p}, {df}) = {q}, want {x}");
        }
        assert_eq!(chi_square_survival(0.0, 3), 1.0);
        assert!(chi_square_survival(1e4, 3) < 1e-100);
    }

    #[test]
    fn test_chi_square_critical_value_is_one_tailed() {
        let ct = ClassicalStatisticalTests::new(0.95);
        // df > 4 used to fall back to a two-sided z = 1.96 approximation.
        assert!((ct.chi_square_critical_value(10, 0.95) - 18.307).abs() < 1e-3);
        assert!((ct.chi_square_critical_value(255, 0.95) - 293.248).abs() < 1e-2);
        assert!((ct.chi_square_critical_value(10, 0.90) - 15.987).abs() < 1e-3);
    }

    #[test]
    fn test_pov_flags_full_embedding_not_cover() {
        let steg = AdvancedSteganalysis::new(0.1);
        // Under full embedding the PoV probability is ~Uniform(0,1), so it is
        // only expected to exceed α = 0.05 in ~95 % of carriers: check several.
        let mut detected = 0;
        for seed in 0..10u64 {
            let cover = stretched_image(128, 128, 100 + seed);
            let stego = sequential_lsb_embed(&cover, 1.0, 200 + seed);
            let c = steg.pairs_of_values_test(&cover).unwrap();
            let s = steg.pairs_of_values_test(&stego).unwrap();
            println!(
                "PoV seed {seed}: cover chi2={:.1} df={} p={:.2e} | stego chi2={:.1} df={} p={:.3}",
                c.chi_square_statistic,
                c.degrees_of_freedom,
                c.embedding_probability,
                s.chi_square_statistic,
                s.degrees_of_freedom,
                s.embedding_probability
            );
            assert!(!c.embedding_detected, "clean cover must not be flagged");
            assert!(c.embedding_probability < 1e-6);
            if s.embedding_detected {
                detected += 1;
            }
        }
        assert!(
            detected >= 8,
            "100% LSB embedding detected in only {detected}/10"
        );
    }

    #[test]
    fn test_spa_estimates_embedding_rate() {
        let steg = AdvancedSteganalysis::new(0.1);
        let spa = |d: &[u8]| steg.sample_pairs_analysis(d, 128, 128, 1).unwrap();
        for seed in 1u64..=8 {
            let cover = natural_image(128, 128, seed);
            let c = spa(&cover);
            let tiny = spa(&sequential_lsb_embed(&cover, 0.01, seed + 10));
            let half = spa(&sequential_lsb_embed(&cover, 0.5, seed + 20));
            let full = spa(&sequential_lsb_embed(&cover, 1.0, seed + 30));
            println!(
                "SPA seed {seed}: cover={:.4} 1%={:.4} 50%={:.4} 100%={:.4}",
                c.raw_estimate, tiny.raw_estimate, half.raw_estimate, full.raw_estimate
            );
            assert!(
                c.raw_estimate.abs() < 0.05,
                "cover estimate {}",
                c.raw_estimate
            );
            assert!(!c.embedding_detected, "cover must not be flagged");
            assert!(
                tiny.estimated_rate < 0.06,
                "~0% estimate {}",
                tiny.estimated_rate
            );
            assert!(!tiny.embedding_detected);
            assert!(
                (half.raw_estimate - 0.5).abs() < 0.1,
                "50% estimate {}",
                half.raw_estimate
            );
            assert!(half.embedding_detected);
            // SPA variance is largest near p = 1 (the discriminant approaches 0).
            assert!(
                (full.raw_estimate - 1.0).abs() < 0.2,
                "100% estimate {}",
                full.raw_estimate
            );
            assert!(full.embedding_detected, "100% embedding must be flagged");
        }

        // A contrast-stretched ("combed") cover violates the SPA cover model and
        // biases the estimate, but full embedding is still clearly separated.
        let cover = stretched_image(128, 128, 3);
        let c = spa(&cover);
        let full = spa(&sequential_lsb_embed(&cover, 1.0, 41));
        println!(
            "SPA stretched: cover={:.4} 100%={:.4}",
            c.raw_estimate, full.raw_estimate
        );
        assert!(!c.embedding_detected);
        assert!(full.estimated_rate > 0.85 && full.embedding_detected);
    }

    #[test]
    fn test_pixel_pair_statistic_varies_and_detects() {
        let steg = AdvancedSteganalysis::new(0.1);
        let cover = natural_image(96, 96, 5);
        let stego = sequential_lsb_embed(&cover, 1.0, 6);
        let other = natural_image(96, 96, 77);
        let rc = steg.pixel_pair_analysis(&cover, 96, 96).unwrap();
        let rs = steg.pixel_pair_analysis(&stego, 96, 96).unwrap();
        let ro = steg.pixel_pair_analysis(&other, 96, 96).unwrap();
        println!(
            "pixel-pair cover: chi2={:.2} p={:.3} rate={:.3}; stego: chi2={:.2} p={:.3e} rate={:.3}",
            rc.chi_square_statistic,
            rc.p_value,
            rc.embedding_probability,
            rs.chi_square_statistic,
            rs.p_value,
            rs.embedding_probability
        );
        // Statistic is a function of the data, not of the pair count.
        assert_ne!(rc.chi_square_statistic, rs.chi_square_statistic);
        assert_ne!(rc.chi_square_statistic, ro.chi_square_statistic);
        assert_ne!(rc.p_value, rs.p_value);
        assert!(!rc.lsb_embedding_detected);
        assert!(rs.lsb_embedding_detected);
        assert!(rs.p_value < 0.05);
    }

    #[test]
    fn test_performance_analysis_detects_full_embedding() {
        let suite = StatisticalValidationSuite::new();
        let cover = natural_image(128, 128, 21);
        let stego = sequential_lsb_embed(&cover, 1.0, 22);
        let res = suite.run_performance_analysis(&cover, &stego).unwrap();
        let roc = res.roc_analysis.unwrap();
        let cv = res.cross_validation.unwrap();
        println!(
            "perf: rate={:.3} samples={} auc={:.3} cv_auc={:.3} cv_acc={:.3}",
            res.simulated_embedding_rate, res.samples, roc.auc, cv.mean_auc, cv.mean_accuracy
        );
        assert!(res.simulated_embedding_rate > 0.9);
        assert!(roc.auc > 0.95, "AUC {}", roc.auc);
        assert!(cv.mean_auc > 0.95, "CV AUC {}", cv.mean_auc);
        assert!(cv.mean_accuracy > 0.9, "CV accuracy {}", cv.mean_accuracy);

        // No embedding: detector cannot separate the classes.
        let res0 = suite.run_performance_analysis(&cover, &cover).unwrap();
        assert_eq!(res0.simulated_embedding_rate, 0.0);
        let auc0 = res0.roc_analysis.unwrap().auc;
        assert!(
            (auc0 - 0.5).abs() < 0.1,
            "AUC at 0% embedding should be ~0.5, got {auc0}"
        );
    }
}
