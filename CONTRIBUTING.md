# Contributing to Stegano

Thank you for your interest in contributing to the Stegano scientific data steganography system! This document provides guidelines for contributing to the project.

## Code of Conduct

This project adheres to a standard of respectful, inclusive collaboration. Please be courteous and professional in all interactions.

## Getting Started

### Prerequisites
- Rust 1.70+ with Cargo
- Git
- (Optional) NVIDIA GPU with CUDA for GPU acceleration features

### Development Environment Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/stegano.git
cd stegano

# Build the project
cargo build

# Run tests
cargo test

# Run examples
cargo run --example basic_demo
```

## Development Guidelines

### Quality Standards (MANDATORY)
- **Overall Coverage**: ≥ 85%
- **Individual Files**: ≥ 80%
- **Security Modules**: ≥ 95% (crypto, handlers, core)
- **Zero Clippy Warnings**: All code must pass `cargo clippy -- -D warnings`
- **Proper Formatting**: All code must pass `cargo fmt --check`

### Code Style
- Follow standard Rust formatting (`cargo fmt`)
- Use `cargo clippy` for linting with zero warnings
- Add documentation for public APIs (100% coverage required)
- Include unit tests for new functionality
- Use conventional commit format for commit messages

### Commit Messages
- Use clear, descriptive commit messages
- Include scope and type (feat, fix, docs, etc.)
- Reference issues when applicable

Example:
```
feat(crypto): add ChaCha20-Poly1305 encryption support

- Implement ChaCha20-Poly1305 as alternative to AES-GCM
- Add performance benchmarks comparing algorithms
- Update API to support algorithm selection

Closes #123
```

### Testing
- Write unit tests for new functionality
- Add integration tests for new handlers
- Include performance benchmarks when relevant
- Ensure all tests pass before submitting PRs

### Documentation
- Update README.md for significant changes
- Add inline documentation for new APIs
- Include examples for new features

## Adding New Format Support

To add support for a new data format:

1. **Create Handler Module**
   ```rust
   // src/handlers/your_format.rs
   use crate::core::*;
   use super::DataHandler;
   use async_trait::async_trait;

   pub struct YourFormatHandler;

   #[async_trait]
   impl DataHandler for YourFormatHandler {
       // Implement required methods
   }
   ```

2. **Add Format Detection**
   - Implement magic byte detection
   - Add format to `DataFormat` enum
   - Update format registry

3. **Implement Embedding Strategy**
   - Design format-specific embedding
   - Implement capacity estimation
   - Add integrity verification

4. **Create Tests and Examples**
   - Add unit tests
   - Create integration tests
   - Provide usage examples

5. **Update Documentation**
   - Add to supported formats list
   - Include API documentation
   - Provide migration guide if needed

## GPU Acceleration

When contributing GPU acceleration features:

1. **WGSL Compute Shaders**
   - Use WGPU for cross-platform compatibility
   - Place shaders in `gpu_kernels/wgsl/`
   - Include performance benchmarks

2. **CUDA Kernels**
   - Implement for NVIDIA-specific optimization
   - Place in `gpu_kernels/cuda/`
   - Ensure fallback to CPU implementation

3. **Performance**
   - Benchmark against CPU implementations
   - Document performance characteristics
   - Include memory usage analysis

## Security Considerations

When working on cryptographic or security features:

- Use established cryptographic libraries
- Implement constant-time operations
- Include security tests and fuzzing
- Document threat model and assumptions
- Follow secure coding practices

## Pull Request Process

1. **Before Submitting**
   - Ensure all tests pass
   - Verify code coverage ≥ 85% (`cargo tarpaulin --fail-under 85`)
   - Run `cargo fmt` and `cargo clippy -- -D warnings`
   - Run security audit (`cargo audit`)
   - Update documentation
   - Add/update tests as needed
   - Ensure pre-commit hooks pass (`pre-commit run --all-files`)

2. **PR Description**
   - Clearly describe the changes
   - Reference related issues
   - Include testing instructions
   - Note any breaking changes

3. **Review Process**
   - Address reviewer feedback
   - Keep commits focused and logical
   - Rebase when requested

## Issue Reporting

When reporting bugs or requesting features:

### Bug Reports
Include:
- Rust version and OS
- Steps to reproduce
- Expected vs actual behavior
- Relevant code examples
- Error messages/logs

### Feature Requests
Include:
- Use case description
- Proposed API design
- Implementation considerations
- Performance implications

## Release Process

Releases follow semantic versioning:
- **Major** (x.0.0): Breaking API changes
- **Minor** (x.y.0): New features, backward compatible
- **Patch** (x.y.z): Bug fixes, backward compatible

### Publishing to npm

The WebAssembly bindings are published to npm as
[`stegano-wasm`](https://www.npmjs.com/package/stegano-wasm) by
`.github/workflows/npm-publish.yml`. **Pushing a `v*` tag publishes it.** The
workflow builds `pkg/` with `scripts/build-wasm.sh`, runs the smoke tests
(`npm/test/run.mjs`) against the packed tarball, and checks that the tag
(without the `v`) equals the version in `Cargo.toml`. It then publishes that
exact tarball with `npm publish --provenance --access public`. If the version
is already on npm, the workflow skips the publish and prints a notice.

One-time setup: add an `NPM_TOKEN` repository secret (Settings → Secrets and
variables → Actions). It must be an npm **automation** token, or a
**granular access token** with read and write permission for `stegano-wasm`.
For the first publish, the token's scope must allow creating new packages.
If the secret is missing, the publish step fails with an error that says so.

To check a release without publishing, run the workflow manually
(**Actions → npm publish → Run workflow**) with `dry_run` on, which is the
default. A real publish from `workflow_dispatch` is only allowed on a tag ref,
for example to retry after adding the secret.

Locally:

```bash
./scripts/build-wasm.sh        # assemble pkg/
node npm/test/run.mjs          # pack + smoke tests
npm pack --dry-run ./pkg       # list the tarball contents
```

## Architecture Guidelines

### Core Principles
- **Modularity**: Keep components loosely coupled
- **Performance**: Optimize for scientific data workflows
- **Security**: Prioritize data protection and integrity
- **Extensibility**: Design for easy format addition

### Design Patterns
- Use traits for extensible interfaces
- Implement async patterns for I/O operations
- Apply RAII for resource management
- Follow Rust ownership patterns

## Questions and Support

- **General Questions**: Open a GitHub discussion
- **Bug Reports**: Create a GitHub issue
- **Security Issues**: Contact maintainers privately
- **Feature Requests**: Create a GitHub issue with enhancement label

## Recognition

Contributors will be recognized in:
- Project README.md
- Release notes
- Git commit history

Thank you for contributing to the advancement of scientific data protection!