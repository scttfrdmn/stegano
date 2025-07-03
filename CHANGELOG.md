# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Format-specific embedding strategies for advanced steganography
- Enhanced signals handler with 5 sophisticated embedding techniques
- Enhanced imagery handler with 5 advanced embedding strategies  
- FFT-based frequency domain embedding for audio signals
- Edge-based embedding for imperceptible image steganography
- Comprehensive strategy selection based on stealth levels
- Improved capacity estimation for different embedding strategies
- Enhanced security with proper encryption payload handling

### Fixed
- SecurePayload serialization/deserialization in handlers
- Capacity calculation errors in bit/byte conversions
- Type safety for strategy enum conversions
- Literal overflow issues in LSB manipulation
- Metal GPU backend compilation on non-macOS platforms

### Changed
- Handlers now support multiple embedding strategies per format
- Improved test coverage for enhanced handlers
- Better error handling for insufficient capacity scenarios

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