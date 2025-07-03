# Stegano Documentation

## Overview

Comprehensive documentation for the stegano scientific steganography library. This documentation covers everything from basic usage to advanced enterprise deployment patterns.

## Documentation Structure

### 📖 [API Reference](api/)
Complete technical documentation for all public APIs and interfaces.

- **[Core Types & Traits](api/core.md)** - Fundamental types, error handling, and core traits
- **[Handler System](api/handlers.md)** - Format-specific steganography handlers
- **[GPU Acceleration](api/gpu.md)** - High-performance GPU computing interface
- **[Cryptography](api/crypto.md)** - Encryption and security features
- **[FFI Integration](api/ffi.md)** - C/C++ foreign function interface

### 🎯 [User Guides](guides/)
Step-by-step tutorials and practical guidance for common use cases.

- **[Getting Started](guides/getting-started.md)** - Quick start guide and basic examples
- **[Performance Optimization](guides/performance-optimization.md)** - GPU acceleration and optimization strategies
- **[Security Best Practices](guides/security-best-practices.md)** - Cryptographic security and threat mitigation
- **[Format-Specific Guides](guides/formats/)** - Domain-specific steganography techniques
- **[Troubleshooting](guides/troubleshooting.md)** - Common issues and solutions

### 🏗️ [Architecture](architecture/)
Design documentation and system architecture details.

- **[System Overview](architecture/overview.md)** - High-level architecture and design principles
- **[Handler System Design](architecture/handler-system.md)** - Extensible format handler architecture
- **[Security Model](architecture/security-model.md)** - Threat model and security architecture
- **[Performance Architecture](architecture/performance.md)** - GPU acceleration and optimization design

### 🔬 [Examples](examples/)
Practical examples demonstrating real-world usage patterns.

- **[Basic Examples](examples/basic/)** - Simple embedding and extraction workflows
- **[Advanced Techniques](examples/advanced/)** - ML-driven optimization and anti-detection
- **[GPU Acceleration](examples/gpu/)** - High-performance computing demonstrations
- **[Scientific Workflows](examples/scientific/)** - Domain-specific processing pipelines

### 🏢 [Enterprise Integration](enterprise/)
Production deployment and enterprise integration guidance.

- **[REST API Server](enterprise/api-server.md)** - HTTP API deployment and usage
- **[Kubernetes Deployment](enterprise/kubernetes.md)** - Container orchestration and scaling
- **[Monitoring & Observability](enterprise/monitoring.md)** - Production monitoring and alerting
- **[Security & Compliance](enterprise/security.md)** - Enterprise security requirements

## Quick Navigation

### For Developers
- New to stegano? Start with [Getting Started](guides/getting-started.md)
- Need performance? Check [GPU Acceleration](api/gpu.md) and [Performance Guide](guides/performance-optimization.md)
- Working with specific formats? See [Format-Specific Guides](guides/formats/)

### For Scientists & Researchers
- **Genomics**: [FASTA/FASTQ Processing](guides/formats/genomics.md)
- **Medical Imaging**: [DICOM and Microscopy](guides/formats/imagery.md)
- **Signal Processing**: [Audio and Timeseries](guides/formats/signals.md)

### For Enterprise Users
- **Production Deployment**: [Enterprise Integration](enterprise/)
- **Security Requirements**: [Security Best Practices](guides/security-best-practices.md)
- **Monitoring & Scaling**: [Monitoring Guide](enterprise/monitoring.md)

## Contributing to Documentation

We welcome contributions to improve the documentation! Please see [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines on:

- Documentation style and formatting
- Example code standards
- Review process for documentation changes
- Building and testing documentation locally

## Documentation Standards

### Code Examples
All code examples in the documentation are:
- ✅ **Tested**: Validated in CI/CD pipeline
- ✅ **Complete**: Include necessary imports and error handling
- ✅ **Current**: Updated with each release
- ✅ **Cross-platform**: Work on Linux, macOS, and Windows

### Versioning
Documentation is versioned alongside the library:
- **Latest**: Always reflects the current development version
- **Stable**: Corresponds to the latest released version
- **Previous**: Archived versions available for reference

### Feedback & Support

- **Issues**: Report documentation issues on [GitHub Issues](https://github.com/stegano/stegano/issues)
- **Discussions**: Ask questions in [GitHub Discussions](https://github.com/stegano/stegano/discussions)
- **Pull Requests**: Contribute improvements via [GitHub Pull Requests](https://github.com/stegano/stegano/pulls)

---

*Documentation last updated: v0.2.0-dev*