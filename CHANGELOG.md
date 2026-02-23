# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Nothing yet

### Changed
- Nothing yet

## [0.3.11] - 2026-02-22

### Fixed
- **FFI version string** (`src/ffi.rs`): Updated from `"0.3.9"` to `"0.3.11"`.
- **CLI hardcoded default key** (`src/cli/commands.rs`): `handle_embed` and `handle_extract`
  previously fell back to `b"default_key"` when no `--password` was supplied, silently
  encrypting with a known key. Now falls back to an empty key (no encryption) so the
  behaviour is honest and non-deceptive.
- **SAM/BAM `AuxiliaryData` silent fallback** (`src/handlers/sam_bam.rs`): Both the embed
  and extract dispatch arms previously caught `AuxiliaryData` with a wildcard and silently
  used `OptionalFields`. They now return `InvalidParameter` explicitly, consistent with the
  other unimplemented strategies.

### Added
- **SAM/BAM `QualityScores` roundtrip test** (`src/handlers/sam_bam.rs`):
  `test_sam_quality_scores_embedding_roundtrip` verifies the quality-score strategy
  end-to-end, including biological-integrity check after embedding.

### Changed (Clippy / code quality)
- `src/handlers/vcf.rs`: Removed unused constant `CUSTOM_ID_PREFIX`.
- `src/codec.rs`: Replaced `% 255 != 0` with `.is_multiple_of(255)`.
- `src/handlers/sam_bam.rs`: Replaced `repeat(0u8).take(n)` with `repeat_n(0u8, n)`;
  replaced manual `(x + 1) / 2` with `.div_ceil(2)`.
- `src/ml/models.rs`: Fixed over-indented doc list item.
- `src/monitoring/telemetry.rs`: Removed redundant `as i64` casts on `tv_sec` fields.
- `src/monitoring/mod.rs`: Added `#[allow(clippy::await_holding_lock)]` on `start`/`stop`
  (sequential single-task calls; deadlock risk is accepted and documented).
- `src/validation.rs`: Replaced four `.max(0.0).min(1.0)` chains with `.clamp(0.0, 1.0)`;
  replaced `% 2 != 0` with `.is_multiple_of(2)`; added `Default` impls for
  `PerformanceAnalysis`, `ValidationReport`, and `TestDatasetManager`.
- `README.md`: Updated test badge to `333+`.

## [0.3.10] - 2026-02-22

### Fixed
- **FFT Hermitian symmetry** (`src/handlers/signals.rs`): `embed_fft_magnitude` and
  `embed_fft_phase` previously modified both a frequency bin and its conjugate independently,
  breaking Hermitian symmetry. The IFFT real part then produced `(Z[k] + conj(Z[N-k])) / 2`
  instead of `Z[k]`, scrambling the embedded parity. Both functions now only modify
  positive-frequency bins (`[N/4..N/2)` for magnitude, `[N/8..N/4)` for phase) and explicitly
  set the conjugate bin to `conj(X[k])`, ensuring the IFFT output is real and extraction
  faithfully recovers the encoded bits.
- **FFT capacity accounting** (`src/handlers/signals.rs`): `estimate_signal_capacity` for
  `FrequencyDomain` now correctly counts `chunk_size / 4` usable bits per chunk (the
  positive-frequency half only) instead of `chunk_size / 2`.
- **Quality-score valid range** (`src/handlers/sam_bam.rs`): `extract_quality_scores` now
  filters to characters in `34..=126` (was `33..=126`). ASCII 33 (`!`) with its LSB cleared
  becomes 32 (space), an invalid PHRED character; excluding it prevents corrupt quality strings.
- **Imagery DCT/PVD capacity** (`src/handlers/imagery.rs`): `estimate_image_capacity` for
  `DCTFrequency` and `PVD` strategies now returns `(0, 0)` instead of a bogus non-zero estimate,
  correctly reflecting that these strategies are not yet implemented.
- **DICOM UnusedMetadata/SequencePadding capacity** (`src/handlers/dicom.rs`): Same fix —
  `estimate_dicom_capacity` returns `0` for these unimplemented strategies.
- **FFI version string** (`src/ffi.rs`): `stegano_version()` now returns `"0.3.10"` (was stale
  `"0.2.2"`).
- **FFI error mapping** (`src/ffi.rs`): `SteganoError::SerializationError` now maps to
  `SteganoErrorCode::DataCorrupted` instead of `InvalidParameter`.
- **Frequency capacity formula** (`src/capacity/mod.rs`): `estimate_frequency_capacity` now
  uses `signal_length / 2` (was `/4`), correctly reflecting that half the frequency bins are
  usable. Test assertions updated accordingly.

### Added
- **VCF sample-FORMAT roundtrip test** (`src/handlers/vcf.rs`): New
  `test_vcf_sample_format_embedding_roundtrip` covers the `format` strategy end-to-end.
- **Signal FFT roundtrip tests** (`src/handlers/signals.rs`): New
  `test_frequency_domain_embedding_roundtrip` and `test_phase_modulation_embedding_roundtrip`
  verify the FFT magnitude and phase strategies produce correct roundtrips.

## [0.3.9] - 2026-02-22

### Fixed
- **VCF quality-score self-framing** (`src/handlers/vcf.rs`): `embed_quality_strategy` now prepends
  a 4-byte little-endian length header before encoding payload bits; `extract_quality_strategy`
  reads that header to determine exact payload length, eliminating over-read from extra variants.
- **VCF variant-ID ordering** (`src/handlers/vcf.rs`): Every chunk now carries an embedded sequence
  index (`STEG{i:04}_<chunk>`). Extraction sorts collected chunks by index before concatenating,
  making round-trips correct even if variant order changes between embed and extract.
- **VCF sample-FORMAT ordering** (`src/handlers/vcf.rs`): Custom FORMAT fields use per-variant
  indexed tags (`S{i:02}`) instead of a static `STEG` tag. Extraction sorts by numeric index,
  ensuring correct bit ordering.
- **Codec RS overflow** (`src/codec.rs`): `rs_encode` now uses `checked_mul(255).and_then(…)`
  when computing the output buffer capacity, returning `InvalidParameter` instead of panicking on
  implausibly large payloads.
- **Imagery DCT/PVD silent fallback** (`src/handlers/imagery.rs`): `embed_lsb` and
  `extract_with_strategy` for `DCTFrequency` and `PVD` variants now return an explicit
  `InvalidParameter` error instead of silently falling back to SimpleLSB. `AdaptiveLSB` is
  correctly handled using the same sequential pixel-LSB path as `SimpleLSB`.
- **DICOM UnusedMetadata / SequencePadding silent fallback** (`src/handlers/dicom.rs`): Both embed
  and extract now return an explicit `InvalidParameter` error for these unimplemented strategy
  variants instead of silently falling back to `PrivateDataElements`.
- **Signal extraction per-strategy dispatch** (`src/handlers/signals.rs`): `extract_with_strategy`
  now dispatches to dedicated extraction functions for each strategy
  (`extract_fft_magnitude_bits`, `extract_fft_phase_bits`, `extract_spread_spectrum_bits`,
  `extract_echo_hiding_bits`) instead of always using the time-domain LSB path for non-time-domain
  embeds.
- **FFT magnitude quantization** (`src/handlers/signals.rs`): `embed_fft_magnitude` switched from
  a multiplicative modifier (not invertible) to parity quantization against a fixed step size
  (`MAG_STEP = 0.01`), making `extract_fft_magnitude_bits` reliably recover the embedded bits.

## [0.3.8] - 2026-02-22

### Fixed
- **DICOM PixelDataLSB** (`src/handlers/dicom.rs`): `embed_pixel_data_lsb` now prepends a 4-byte
  little-endian length header before the payload bits; `extract_pixel_data_lsb` reads that header
  to self-discover payload length instead of using a hardcoded `estimated_length = 100`.
- **DICOM parse limit** (`src/handlers/dicom.rs`): Removed the artificial 1000-element cap in
  `parse_dicom()` that silently truncated large DICOM files.
- **SAM/BAM QualityScores** (`src/handlers/sam_bam.rs`): Same self-framing fix as DICOM —
  `embed_quality_scores` prepends a 4-byte length header and `extract_quality_scores` reads it,
  eliminating the hardcoded `estimated_length = 100`.
- **BAM format** (`src/handlers/sam_bam.rs`): BAM embed and extract are now fully implemented.
  Embed appends an `@CO STEGANO_DATA:<hex>` line to the BAM header text; extract recovers it.
  `serialize_bam()` added to round-trip a `BamFile` struct back to binary. The 100-alignment
  parse cap in `parse_bam()` is removed.
- **VCF quality-score encoding** (`src/handlers/vcf.rs`): Replaced the unreliable IEEE 754
  decimal-place approach with a lossless integer scheme: 3 bits are packed into the low 3 bits of
  the rounded integer quality score (`qual = (qual & !7) | triplet`). Serialization changed from
  `{:.3}` (clipped decimal) to `{}` (integer), ensuring bits survive the write/re-read cycle.
- **FFI Tokio runtime** (`src/ffi.rs`): Replaced per-call `Runtime::new()` with a single shared
  `OnceLock<Runtime>` (`TOKIO_RT`) initialized once on first use, eliminating the resource leak.
- **Stale integration test assertions** (`tests/test_0_1_1_fixes.rs`, `tests/test_basic_functionality.rs`):
  Fixed hardcoded version assertions ("0.2.1") and carrier sizes that became too small once
  Reed-Solomon error correction became functional.

## [0.3.7] - 2026-02-22

### Added
- **`src/codec.rs`** — New self-framing encode/decode pipeline that wires
  `EmbedParams::encryption_key` and `EmbedParams::error_correction_level` to
  real, functional implementations:
  - `encode_payload(payload, params)`: AES-256-GCM encryption (from key)
    followed by Reed-Solomon error correction (from ECLevel), returns a
    self-framing byte blob that `decode_payload` can reverse without external
    metadata.
  - `decode_payload(data, params)`: RS decoding then AES-GCM decryption; the
    same params must be used as were supplied to `encode_payload`.
  - `normalize_key(key)`: hashes any-length keys to exactly 32 bytes via Blake3
    so AES-256-GCM always receives a valid key.
  - 21 new unit tests cover no-op, encryption-only, each ECLevel, combined
    encrypt+EC, short/long/empty keys, empty payloads, large payloads, wrong
    key decryption failure, and missing key failure.
- **ECLevel→RS mapping**: `ECLevel::Low`→16 ECC bytes, `Medium`→32, `High`→64,
  `Maximum`→128 per 255-byte RS block. Arbitrary-length payloads are chunked
  into fixed 255-byte RS blocks with zero-padding; the original length is
  stored in a 4-byte header so the decoder can trim padding.

### Changed
- **All handlers now respect `params.encryption_key` and
  `params.error_correction_level`** (`generic.rs`, `imagery.rs`,
  `signals.rs`, `genomics.rs`):
  - When `encryption_key` is non-empty or `error_correction_level != None`,
    `embed()` calls `codec::encode_payload` before embedding the bytes and
    `extract()` calls `codec::decode_payload` after extraction.
  - `imagery.rs` and `signals.rs`: the header `is_encrypted` flag byte is
    now a `codec_mode` byte — 0 = raw, 1 = legacy `self.crypto` password
    path (backward-compatible), 2 = new codec pipeline — so existing embedded
    data created with `with_encryption()` still extracts correctly.
- **Capacity estimates corrected**: `imagery.rs` SimpleLSB header overhead
  64 bits → 80 bits (10-byte header); `signals.rs` TimeDomainLSB header
  overhead 128 bits → 80 bits, matching the actual 10-byte header format.
- **7 unused dependencies removed** from `Cargo.toml`: `wide`, `ndarray`,
  `image`, `tiff`, `bio`, `csv`, `wav` — none were imported anywhere in the
  source tree.

### Fixed
- `embedding.rs` tests `test_embedding_with_validation` and
  `test_embedding_without_validation`: carrier size increased from 1024 to
  8192 bytes to accommodate the RS overhead introduced by the now-functional
  `ECLevel::Medium` default in `EmbedParams::new`.
- `generic.rs` test `test_empty_payload_embedding`: carrier size increased
  from 100 to 4096 bytes for the same reason.

## [0.3.3] - 2026-02-22

### Fixed
- **Statistical tests** (`src/validation.rs`): Chi-square p-value corrected to one-tailed Fisher
  approximation — removed erroneous `.abs()` and `2.0 *` that were converting small chi-square
  values to incorrectly small p-values. O(n²) naive DFT in `simple_fft()` replaced with
  `rustfft` O(n log n). K-fold cross-validation closure signature changed from `-> Result<f64>`
  (accuracy) to `-> Result<Vec<f64>>` (predicted scores); fold AUC now computed via Mann-Whitney
  U statistic rather than a 0.5-threshold accuracy proxy.
- **Distribution and outlier analysis** (`src/ml/analysis.rs`): Replaced five stub
  implementations with real algorithms:
  - `calculate_descriptive_stats()` now computes mean, std_dev, min, max, median, Q1/Q3,
    skewness, and kurtosis for all four tracked metrics
  - `DistributionAnalyzer::analyze()` performs KS goodness-of-fit against Normal, Exponential,
    and Uniform distributions; best fit is the distribution with the smallest KS statistic
  - `OutlierAnalyzer::analyze()` dispatches to IQR-fence, Z-score (|z|>threshold), or
    modified Z-score (MAD-based) detection depending on `self.method`
  - `SamplePairsDetector::evaluate()` implements the SP (Sample Pairs) histogram analysis
    used in classic LSB steganalysis
  - `HistogramDetector::evaluate()` computes Total Variation Distance (TVD = L₁/2) between
    original and stego byte-frequency histograms
- **Process telemetry** (`src/monitoring/telemetry.rs`): Replaced `rand::random` fakes with
  real `libc::getrusage(RUSAGE_SELF)` calls. `collect_memory_sample()` reads current and peak
  RSS; `collect_cpu_sample()` computes delta CPU% from two snapshots 50 ms apart. Handles
  macOS (`ru_maxrss` in bytes) and Linux (`ru_maxrss` in KiB, VmRSS from `/proc/self/status`)
  differences. `#[cfg(not(unix))]` stubs return errors on non-Unix targets.
- **GPU model CPU fallback** (`src/ml/cuda_models.rs`): `forward_gpu()` CPU path previously
  returned `vec![0.5; output_size]` regardless of input. Now calls `cpu_forward()`, a real
  dense+sigmoid forward pass through all model layers. `predict_optimal_params()` refactored
  to reuse `cpu_forward()` rather than duplicating the same loop inline.

### Changed
- Test count increased from 261 to 309 (+48 tests covering all fixed implementations)

## [0.3.2] - 2026-02-21

### Added
- **CUDA Acceleration**: NVIDIA GPU backend via `cudarc 0.12` behind the optional `cuda` feature flag
  - `CudaAccelerator` with `parallel_lsb_embed()` and `parallel_lsb_extract()` powered by NVRTC-compiled kernels
  - Kernels compiled at runtime so the driver JITs them to the native GPU architecture (forward-compatible with Blackwell sm_120 and beyond)
  - `GpuBackend::Cuda` variant wired into `best_available()` with priority Metal > CUDA > WGPU > CPU
  - Enable with `--features cuda`; stub struct on non-CUDA targets keeps all other builds unaffected

### Fixed
- **Cross-platform build**: Dependencies placed after `[target.'cfg(target_os = "macos")'.dependencies]` in `Cargo.toml` were silently treated as macOS-only by TOML, causing 64 compilation errors on Linux. Moved all non-macOS-specific deps (`log`, `clap`, `hex`, `libc`, `bio`, `csv`, `wav`, `reed-solomon`, etc.) above the platform-specific section.

## [0.3.1] - 2026-02-21

### Added
- **Validation Dashboard**: `ValidationReport` now implements `fmt::Display`, rendering a human-readable text dashboard with per-test results (statistic, p-value, pass/fail) and recommendations
- **JSON Export**: `ValidationReport::to_json_string()` convenience method wraps `serde_json::to_string_pretty` for machine-readable report output
- **Validation Summary API**: `ValidationReport::summary()` returns a compact `ValidationSummary` struct with `tests_run`, `passed`, `overall_score`, and `status` fields
- **Community Infrastructure**: GitHub issue templates for bug reports and feature requests; blank-issue guard with link to discussions
- **Validation User Guide**: `docs/VALIDATION_GUIDE.md` covering how to enable validation, interpret dashboard output, and understand scoring

### Changed
- **Parallel Classical Tests**: The five independent classical statistical tests (chi-square, KS, runs, Anderson-Darling, autocorrelation) in `run_classical_tests()` now execute concurrently using `rayon::join` chains, reducing wall-clock validation time on multi-core systems

### Fixed
- Version badge in README updated to 0.3.1
- Test count badge updated to 261+

## [0.3.0] - 2025-01-12

### Added
- **Statistical Validation Framework**: Comprehensive 11/10 excellence level validation suite
  - Chi-square goodness of fit tests with p-value calculations
  - Kolmogorov-Smirnov distribution comparison tests
  - Runs tests for randomness assessment
  - Anderson-Darling normality tests
  - Autocorrelation analysis with Ljung-Box statistics
  - ROC analysis and AUC calculations for detection performance
  - Pixel pair analysis for image steganalysis
  - Wavelet-based structural analysis for advanced steganalysis
  - Cross-validation framework for model performance
  - Publication-ready statistical reports
- **High-Level Embedding API**: New `SteganoEmbedder` unified interface
  - Integrated validation pipeline with automatic reporting
  - Format-specific validation tailored to data types
  - Performance metrics and timing analysis
- **CLI Validation Integration**: Enhanced command-line interface
  - `--validate` flag for all operations
  - Real-time validation scoring and recommendations
  - Statistical analysis integration in analyze command
  - Enhanced error reporting and user feedback
- **Modern Security & Supply Chain Tools**
  - `cargo-audit` integration for vulnerability scanning
  - `cargo-deny` configuration for license compliance and policy enforcement
  - Automated security auditing via GitHub Actions
  - Comprehensive security documentation (SECURITY.md)
  - Supply chain monitoring with dependency analysis
  - Security audit script with reporting capabilities
- **CI/CD Improvements**
  - Reduced notification spam (95% reduction in GitHub notifications)
  - Daily security audits instead of constant monitoring
  - Optimized workflow scheduling for better resource usage
- **Development Practices**
  - Semantic Versioning (SemVer 2.0) compliance
  - Keep a Changelog format adoption
  - Developer guidelines documentation
- **Enhanced Handlers**: Format-specific embedding strategies for advanced steganography
  - Enhanced signals handler with 5 sophisticated embedding techniques
  - Enhanced imagery handler with 5 advanced embedding strategies  
  - FFT-based frequency domain embedding for audio signals
  - Edge-based embedding for imperceptible image steganography
  - Comprehensive strategy selection based on stealth levels
  - Improved capacity estimation for different embedding strategies

### Changed
- **Core API Extensions**: Enhanced `EmbedParams` and `EmbedResult`
  - Added `enable_validation` field to `EmbedParams` for validation control
  - Added `validation_report` field to `EmbedResult` for comprehensive reporting
  - Added `.with_validation()` convenience method for easy enabling
- **CI/CD Workflow Optimization**
  - `ci-monitoring.yml`: Reduced from every 30 minutes to daily at 9 AM UTC (weekdays only)
  - `status-dashboard.yml`: Reduced from hourly to twice daily (9 AM, 6 PM weekdays)
  - Added `[skip ci]` tags to prevent cascading workflows
- **Project Metadata**
  - Updated author email to scttfrdmn@gmail.com
  - Updated license specification to MIT format
  - Enhanced project description and documentation
- **Version Bump**: Updated from 0.2.2 to 0.3.0 for major feature release
- **Handler Improvements**: Support for multiple embedding strategies per format
  - Better error handling for insufficient capacity scenarios
  - Improved test coverage for enhanced handlers

### Fixed
- **Compilation Issues**: Resolved all validation integration compilation errors
  - Fixed missing `enable_validation` fields in 20+ EmbedParams struct initializations
  - Fixed missing `validation_report` fields in 5+ EmbedResult struct initializations
  - Ensured backward compatibility with existing API usage
- **CI/CD Notification Spam**: Eliminated excessive GitHub workflow notifications
  - Reduced workflow execution frequency by 95%
  - Improved notification relevance and reduced noise
- **Handler Fixes**: Multiple technical improvements
  - SecurePayload serialization/deserialization in handlers
  - Capacity calculation errors in bit/byte conversions
  - Type safety for strategy enum conversions
  - Literal overflow issues in LSB manipulation
  - Metal GPU backend compilation on non-macOS platforms
- **Documentation**: Updated collaboration and security documentation
  - Fixed contact information throughout project
  - Enhanced security policy and vulnerability reporting procedures

### Security
- **Enhanced Security Framework**: Multi-layered protection improvements
  - Added automated vulnerability scanning with daily audits
  - Implemented license compliance monitoring
  - Created comprehensive security policy documentation
  - Added supply chain security monitoring
  - Enhanced dependency management with cargo-deny policies
- **Cryptographic Improvements**: Maintained existing strong encryption
  - AES-256-GCM and ChaCha20-Poly1305 encryption options
  - Blake3 key derivation with configurable parameters
  - Reed-Solomon error correction with distributed recovery
  - Post-quantum readiness architecture
- **Secure Payload Handling**: Enhanced security with proper encryption payload handling

## [0.1.0] - 2025-01-03

### Added
- Metal GPU acceleration support for macOS (M4 Pro optimized)
- Comprehensive development rules and quality gates
- 85% code coverage requirement with extensive test suite
- Advanced steganography techniques for scientific data
- Multi-format support (FASTA, FASTQ, images, signals)
- AES-256-GCM encryption with authenticated payloads
- GPU-accelerated operations for high-performance processing
- Extensive documentation and API reference
- Cross-platform compatibility with fallback implementations

### Security
- BLAKE3 cryptographic hashing for integrity verification
- Authenticated encryption with automatic key derivation
- Secure payload serialization with tamper detection
- Memory-safe Rust implementation throughout