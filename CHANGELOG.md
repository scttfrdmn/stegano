# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Nothing yet

### Changed
- Nothing yet

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