# Stegano Validation Guide

This guide explains how to use the statistical validation framework introduced in v0.3.0 and enhanced in v0.3.1.

## Enabling Validation

Validation is opt-in. Enable it by calling `.with_validation()` on your `EmbedParams` before embedding:

```rust
use stegano::core::{EmbedParams, DataFormat};
use stegano::embedding::SteganoEmbedder;

let params = EmbedParams::new(DataFormat::PNG)
    .with_validation();          // enable the validation pipeline

let embedder = SteganoEmbedder::new();
let result = embedder.embed(&cover_data, &payload, params)?;

if let Some(report) = result.validation_report {
    // print the dashboard
    println!("{report}");
}
```

When validation is disabled (the default), no statistical tests are run and the `validation_report` field on `EmbedResult` is `None`.

---

## Reading the Dashboard

Calling `println!("{report}")` on a `ValidationReport` renders a text dashboard:

```
╔══════════════════════════════════════╗
║      Stegano Validation Dashboard    ║
╚══════════════════════════════════════╝
Tests completed: 5  |  Status: PASS

Classical Tests
  chi_square  χ²=4.231  p=0.834  [ OK ]
  ks_test     D=0.021   p=0.912  [ OK ]
  runs_test   z=0.453   p=0.651  [ OK ]
  anderson_d  A²=0.312  p=0.571  [ OK ]
  autocorr    LB=8.142  p=0.419  [ OK ]

Recommendations
  • Data distribution appears uniform — low detectability risk
```

### Status values

| Status | Meaning |
|--------|---------|
| `PASS` | All 5 classical tests pass (no statistically significant anomaly detected) |
| `WARN` | 3 or 4 tests pass — review the failing tests and recommendations |
| `FAIL` | Fewer than 3 tests pass — embedding is likely detectable |

---

## Understanding Each Test

### Chi-square (χ²)
Compares the byte-frequency histogram of the stego data to the original. A high p-value (> 0.05) means the distributions are statistically indistinguishable — good for stealth.

- **χ²**: Test statistic; smaller is better (closer to original distribution)
- **p**: p-value; higher means less detectable

### Kolmogorov-Smirnov (KS)
Measures the maximum difference between the empirical CDFs of the original and stego data. A low D statistic and high p-value indicate the distributions match closely.

- **D**: Maximum CDF difference; smaller is better
- **p**: p-value for the two-sample test

### Runs test
Tests whether the LSB sequence of the stego data looks random. Steganographic embedding that flips LSBs in a non-random pattern can be detected here.

- **z**: Z-score of the runs count; near 0 is ideal
- **p**: p-value; above 0.05 means the sequence appears random

### Anderson-Darling (A²)
Tests whether the stego byte values follow a normal distribution. Some embedding techniques introduce non-Gaussian artifacts.

- **A²**: Test statistic; smaller indicates closer to normal
- **p**: p-value; above 0.05 means no significant departure from normality

### Autocorrelation (Ljung-Box)
Checks for temporal dependencies in the stego data. Periodic or patterned embedding leaves autocorrelation signatures that this test detects.

- **LB**: Ljung-Box statistic across up to 20 lags
- **p**: p-value; above 0.05 means no significant autocorrelation detected

---

## Overall Score

`ValidationReport::overall_score` is a 0–100 composite score set by the embedding pipeline. A score of 0 means the field has not been populated by the embedder (most embedders currently leave it at 0; check recommendations instead).

Use `report.summary()` for a quick programmatic check:

```rust
let s = report.summary();
println!("Passed {}/{} tests — status: {}", s.passed, s.tests_run, s.status);
```

---

## JSON Export

For logging or downstream tooling, serialize the full report to JSON:

```rust
let json = report.to_json_string();
std::fs::write("validation_report.json", &json)?;
```

The JSON structure mirrors the `ValidationReport` struct fields and is stable across patch versions.

---

## Example: Running the Showcase

The repository includes a ready-to-run example:

```bash
cargo run --example validation_showcase
```

Expected output includes the full dashboard for a sample PNG embedding, followed by JSON output and summary statistics.

---

## Performance Notes

As of v0.3.1, the five classical tests run in parallel using Rayon's work-stealing thread pool (`rayon::join` chains). On a quad-core machine, wall-clock validation time is roughly 2–3× faster than the sequential baseline for typical data sizes (≥ 4 KB). The thread pool size follows `RAYON_NUM_THREADS` if set.
