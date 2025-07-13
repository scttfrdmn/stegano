# Security Policy

## Supported Versions

We actively support and provide security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.3.x   | ✅ Full support    |
| 0.2.x   | 🟡 Critical fixes only |
| < 0.2   | ❌ Not supported   |

## Reporting a Vulnerability

If you discover a security vulnerability in Stegano, please report it to us responsibly:

### For Critical Security Issues
- **Email**: scttfrdmn@gmail.com with subject "SECURITY: [Brief Description]"
- **Response Time**: We aim to respond within 24 hours
- **Disclosure**: We follow coordinated disclosure practices

### What to Include
Please include the following information in your report:
- Description of the vulnerability
- Steps to reproduce the issue
- Potential impact assessment
- Suggested fix (if known)
- Your contact information for follow-up

### What to Expect
1. **Acknowledgment**: We'll acknowledge receipt within 24 hours
2. **Initial Assessment**: We'll provide an initial assessment within 72 hours
3. **Regular Updates**: We'll keep you informed of our progress
4. **Resolution**: We aim to resolve critical issues within 7 days
5. **Disclosure**: We'll coordinate public disclosure timing with you

## Security Features

### Cryptographic Security
- **Encryption**: AES-256-GCM (primary), ChaCha20-Poly1305 (alternative)
- **Key Derivation**: Blake3 with configurable parameters
- **Random Number Generation**: OS-provided secure random sources
- **Post-Quantum Readiness**: Architecture prepared for NIST algorithms

### Data Integrity
- **Cryptographic Hashing**: Blake3 for tamper detection
- **Error Correction**: Reed-Solomon encoding with configurable protection levels
- **Chain of Trust**: Hierarchical integrity verification
- **Distributed Checksums**: Segment-level verification throughout payload

### Supply Chain Security
- **Dependency Auditing**: `cargo-audit` for vulnerability scanning
- **License Compliance**: `cargo-deny` for license and policy enforcement
- **Automated Scanning**: Daily security audits via GitHub Actions
- **Minimal Dependencies**: Carefully curated dependency tree

### Secure Development Practices
- **Code Review**: All changes require review
- **Automated Testing**: 275+ tests with 100% pass rate
- **Static Analysis**: Clippy and other linting tools
- **Continuous Integration**: Automated security checks on all PRs

## Supply Chain Security

### Dependency Management
We maintain strict control over our dependency tree:
- Regular audits with `cargo-audit`
- License compliance verification
- Minimal attack surface through selective dependencies
- Automated vulnerability monitoring

### Build Security
- Reproducible builds when possible
- Verified dependency sources
- Secure CI/CD pipeline
- Code signing for releases

## Security Testing

### Automated Testing
- Daily vulnerability scans
- Dependency audits on every commit
- Fuzz testing for critical components
- Integration security tests

### Manual Testing
- Regular security code reviews
- Penetration testing for critical releases
- Third-party security audits (planned for v1.0)

## Known Security Considerations

### Steganography-Specific Risks
- **Traffic Analysis**: While Stegano hides data existence, unusual file modifications may be detectable through statistical analysis
- **Format Constraints**: Some data formats have inherent limitations that may affect hiding capacity or detectability
- **Side Channels**: Processing time and memory usage patterns may provide information about hidden data

### Operational Security
- **Key Management**: Users are responsible for secure key generation and storage
- **Access Control**: File system permissions and access controls are outside Stegano's scope
- **Network Security**: Transmission security is the user's responsibility

## Security Updates

### Update Process
1. Security issues are prioritized for immediate fixing
2. Updates are released as patch versions (e.g., 0.3.1 → 0.3.2)
3. Security advisories are published for all security releases
4. Users are notified through GitHub releases and security advisories

### Automatic Updates
We recommend:
- Enabling GitHub security notifications for this repository
- Using automated dependency update tools like Dependabot
- Subscribing to our security mailing list (planned)

## Best Practices for Users

### Secure Usage
- Generate cryptographically strong encryption keys
- Use unique keys for different operations
- Enable validation for critical operations
- Regularly update to the latest version

### Operational Security
- Verify file integrity before and after operations
- Use secure deletion for temporary files
- Implement proper access controls
- Monitor for unusual system behavior

### Development Integration
- Pin specific versions in production
- Regularly audit your dependency tree
- Enable all security features
- Follow the principle of least privilege

## Security Tools and Commands

### Quick Security Check
```bash
# Run comprehensive security audit
./scripts/security_check.sh

# Individual tools
cargo audit                    # Vulnerability scan
cargo deny check              # Policy enforcement
cargo outdated               # Dependency updates
```

### Continuous Monitoring
```bash
# Add to your CI/CD pipeline
cargo audit --deny warnings
cargo deny check advisories
cargo deny check licenses
```

## Contact Information

**Security Team**: scttfrdmn@gmail.com  
**PGP Key**: Available on request  
**Security Policy**: This document  
**Bug Bounty**: Not currently available  

## Acknowledgments

We thank the following for their contributions to Stegano's security:
- The Rust security working group
- Contributors who have reported security issues
- The broader security research community

---

Last updated: January 2025  
Next review: April 2025