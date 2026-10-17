# Validation guide

`stegano::validation` provides statistical tests and LSB steganalysis
detectors. You can use them to estimate how detectable an embedding is on
your own data. They are measurement tools. A low score from these detectors
does not mean the embedding is undetectable: stronger steganalysis exists,
and every stegano format carries a fixed marker that a reader who knows the
scheme can check directly (see [FORMATS.md](FORMATS.md) and
[the threat model and limitations](../SECURITY.md#threat-model-and-limitations)).

## Detectors

### Pairs of values (Westfeld and Pfitzmann, 1999)

`AdvancedSteganalysis::pairs_of_values_test(samples) -> PairsOfValuesResult`

LSB replacement with random message bits evens out the histogram counts
within each value pair `(2k, 2k+1)`. The test computes
`χ² = Σ (h[2k] − h[2k+1])² / (h[2k] + h[2k+1])` over pairs with at least 10
samples and reports:

| Field | Meaning |
|-------|---------|
| `chi_square_statistic`, `degrees_of_freedom`, `pairs_used` | The test itself. |
| `embedding_probability` | `P(χ²_df ≥ statistic)`. Close to 0 means the histogram has the pair asymmetry of a typical cover. High values mean it looks pair-equalised, as after full LSB replacement. |
| `embedding_detected` | `embedding_probability > 0.05` and at least two pairs used. |

Limitations: the test is blind to covers whose histogram is already smooth at
the 1-LSB scale (they look "embedded"), and it only sees embedding that
covers most of the analysed samples. For sequential embedding, run it on
growing prefixes of the data.

### Sample pairs analysis (Dumitrescu, Wu and Wang, 2003)

`AdvancedSteganalysis::sample_pairs_analysis(samples, width, height, channels) -> SamplePairsResult`

SPA classifies pairs of adjacent samples (horizontal neighbours, plus
vertical ones when `height > 1`) and solves for the LSB-replacement rate.
For 1-D signals pass `height = 1` and `width = samples.len()`.

| Field | Meaning |
|-------|---------|
| `estimated_rate` | Estimated fraction of samples carrying message bits, clamped to `[0, 1]`. Clean natural covers usually give a few percent; 100 % embedding gives about 1. |
| `raw_estimate` | The unclamped root. Use it as a continuous score. |
| `balance_chi_square`, `balance_p_value` | Test of the clean-cover assumption that the X and Y pair counts are equal. A small p-value is evidence of embedding. |
| `embedding_detected` | `estimated_rate` above the configured threshold (0.1 in `StatisticalValidationSuite::new()`) and `balance_p_value < 0.05`. |

`pixel_pair_analysis(image, width, height)` is a wrapper around SPA for raw
8-bit images. It infers the channel count from `image.len() / (width × height)`.

Both detectors target LSB replacement on 8-bit samples. They say nothing
about the plaintext channels (FASTA headers, VCF/SAM tags, DICOM private
elements), which are visible without any statistics.

```rust
use stegano::validation::AdvancedSteganalysis;

// A smooth 8-bit ramp as a stand-in for real samples.
let cover: Vec<u8> = (0..65_536u32).map(|i| ((i / 7) % 256) as u8).collect();
let detectors = AdvancedSteganalysis::new(0.1);

let pov = detectors.pairs_of_values_test(&cover)?;
let spa = detectors.sample_pairs_analysis(&cover, cover.len(), 1, 1)?;
println!("PoV p = {:.3}, SPA rate = {:.3}", pov.embedding_probability, spa.estimated_rate);
# Ok::<(), stegano::SteganoError>(())
```

## Validation during embedding

`EmbedParams::with_validation()` (CLI: `--validate`) makes
`SteganoEmbedder::embed` attach a `ValidationReport` to the result. It needs
the whole carrier in memory, so the CLI uses the in-memory path. The checks
run on the carrier's **file bytes** (`CarrierData::raw_data`), before and
after embedding:

| `test_results()` key | Contents |
|---------------------|----------|
| `pairs_of_values_test` | `PairsOfValuesResult` on the modified bytes. |
| `ks_test` | Two-sample Kolmogorov-Smirnov test, original vs modified byte distribution. A small p-value means the distributions differ. |
| `pixel_pair_analysis` | PNG/TIFF/MICROSCOPY: SPA, using `carrier.dimensions` when present. |
| `audio_steganalysis` | WAV/BinarySignal: spectral features at `carrier.sampling_rate`. |
| `roc_analysis` | SPA scores per segment of the modified bytes, labelled by whether the segment changed. Present only when there are at least two changed and two unchanged segments. An AUC near 0.5 means SPA cannot tell changed segments from unchanged ones; near 1.0 means it can. |

```rust,no_run
use stegano::carrier::build_carrier;
use stegano::{EmbedParams, SteganoEmbedder};

#[tokio::main]
async fn main() -> stegano::Result<()> {
    let mut carrier = build_carrier("image.png", std::fs::read("image.png")?);
    let params = EmbedParams::new(b"correct horse battery staple".to_vec()).with_validation();
    let result = SteganoEmbedder::new().embed(&mut carrier, b"secret", &params).await?;

    if let Some(report) = &result.validation_report {
        for (name, value) in report.test_results() {
            println!("{name}: {value}");
        }
    }
    Ok(())
}
```

Points to keep in mind when reading this report:

- **File bytes, not samples.** For compressed containers (PNG, BAM,
  compressed TIFF) the file bytes are compressed data, not pixel values or
  records, so these scores say little about sample-level detectability.
  Decode the samples yourself and call the detectors directly.
- **`overall_score` covers PoV and SPA only.** See
  [Detection-resistance score](#detection-resistance-score) below.
- **The classical section is not filled in here.** The `Display` dashboard
  and `summary()` read `classical_results`, which only
  `StatisticalValidationSuite::comprehensive_validation` populates. For a
  report from `SteganoEmbedder` they show placeholder values.

## Detection-resistance score

`ValidationReport::finalize()` sets `overall_score` (an `Option<f64>` in
`[0, 10]`) from the two LSB-replacement detectors in `steganalysis_results`,
using `stegano::validation::detection_resistance_score`. Both the embedding
path and `comprehensive_validation` call it. Each detector that ran gives a
suspicion in `[0, 1]`:

- pairs of values: 1 if it flags the carrier (`embedding_detected`), else 0;
- SPA: its estimated embedding rate (`pixel_pair_analysis.embedding_probability`).

```text
score = 10 × (1 − mean suspicion)
```

10 means neither detector flags the carrier and SPA estimates no embedding.
The score falls as detectors flag it and as SPA's rate estimate rises, and is
0 when PoV flags it and SPA estimates a 100% rate. `overall_score()` rounds it
to an integer `0..=10`. The CLI (`embed --validate`) prints it as
`Detection-resistance score: N/10`. When neither detector ran (for example,
`comprehensive_validation` on `DataFormat::BINARY`, which runs no PoV/SPA) the
score is `None` and the CLI prints `n/a`.

The score is a heuristic, not a security guarantee:

- It covers only two classical detectors of LSB *replacement*. It says nothing
  about RS analysis, calibrated or machine-learned steganalysis, or embedding
  that does not replace LSBs.
- PoV misses about 5% of fully embedded carriers, because its p-value is
  roughly uniform under the stego hypothesis. A fully embedded carrier can
  therefore score around 5 (SPA alone) instead of around 0.
- PoV also flags covers whose histogram is already smooth at the 1-LSB scale,
  so such a cover can score low with nothing embedded.
- On compressed containers the detectors see compressed bytes (see above).

In the unit tests, three contrast-stretched synthetic 128×128 covers score
10.0, 10.0 and 9.89. The same covers with 100% sequential LSB replacement score
0.06, 5.46 (the PoV miss) and 0.00.

## Full suite

`StatisticalValidationSuite::comprehensive_validation(original, stego, &format)`
runs, on two byte slices:

- **Classical tests** (`classical_results`): chi-square on 10-bin histograms
  of original vs stego, two-sample Kolmogorov-Smirnov, a runs test on the
  stego LSB sequence, Anderson-Darling normality, and Ljung-Box
  autocorrelation over 20 lags. `summary()` counts how many do not reject
  (`PASS` = all 5, `WARN` = 3 or 4, `FAIL` = fewer), and `Display` prints them
  as a dashboard. These are generic distribution tests. Passing them is not
  evidence of security.
- **Steganalysis** (`steganalysis_results`): for PNG/TIFF, SPA and
  pairs-of-values on the bytes as one row, plus a wavelet energy check. For
  WAV, the audio features. Otherwise the wavelet check only.
- **Performance analysis** (`performance_results`): the original is cut into
  about 40 segments. Each gives a cover sample and a simulated LSB-replacement
  stego sample at the observed embedding rate. SPA scores them, and the
  results give a ROC curve and k-fold cross-validated accuracy (threshold fit
  on the training folds only). This estimates how well SPA would detect LSB
  replacement at that rate on this kind of data. It does not test the actual
  strategy used.
- **Domain checks** (`domain_results`).

`report.to_json_string()` serialises everything.

```rust
use stegano::{DataFormat, StatisticalValidationSuite};

let original: Vec<u8> = (0..16_384u32).map(|i| ((i / 5) % 256) as u8).collect();
let mut stego = original.clone();
for (i, b) in stego.iter_mut().enumerate() {
    *b = (*b & !1) | ((i * 7 % 3) as u8 & 1); // crude LSB overwrite
}

let report = StatisticalValidationSuite::new()
    .comprehensive_validation(&original, &stego, &DataFormat::BINARY)?;
let s = report.summary();
println!("{}/{} classical tests passed: {}", s.passed, s.tests_run, s.status);
# Ok::<(), stegano::SteganoError>(())
```

The five classical tests run in parallel with Rayon (`RAYON_NUM_THREADS`
controls the pool size).
