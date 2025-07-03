# Stegano Development Rules

This document establishes mandatory development standards and quality gates for the Stegano scientific data steganography project.

## 🎯 Quality Standards

### Code Coverage Requirements

**MANDATORY COVERAGE THRESHOLDS:**
- **Overall Project Coverage**: ≥ 85%
- **Individual File Coverage**: ≥ 80%
- **Critical Security Modules**: ≥ 95% (crypto, handlers, core)
- **Public API Coverage**: 100%

**Coverage Scope:**
- Line coverage: Primary metric
- Branch coverage: Secondary verification
- Function coverage: Complete coverage required

**Exemptions:**
- GPU shader files (`.wgsl`, `.cu`)
- Example code in `examples/`
- Build scripts and tooling
- Generated code

### Pre-commit Requirements

**AUTOMATED CHECKS (BLOCKING):**
1. **Code Coverage Verification**
   - Run coverage analysis
   - Enforce threshold compliance
   - Generate coverage reports

2. **Code Quality Linting**
   - Rust: `cargo clippy` with deny warnings
   - Format: `cargo fmt` verification
   - Security: `cargo audit` vulnerability check

3. **File Type Linting**
   - YAML: `yamllint`
   - JSON: `jsonlint` 
   - Markdown: `markdownlint`
   - TOML: `taplo` formatter
   - Shell scripts: `shellcheck`

4. **Common Checks**
   - No merge conflict markers
   - No trailing whitespace
   - End-of-file newlines
   - Large file prevention (>100MB)
   - Secrets detection

## 🔧 Tooling Setup

### Code Coverage Tools

```toml
# Add to Cargo.toml [dev-dependencies]
tarpaulin = "0.27"
coverage-tools = "0.2"
```

**Configuration in `.tarpaulin.toml`:**
```toml
[tool.tarpaulin]
exclude = ["examples/*", "gpu_kernels/*", "scripts/*"]
exclude-files = ["**/tests/*", "**/benches/*"]
count = true
line = true
branch = true
out = ["Html", "Lcov", "Json"]
output-dir = "target/coverage"
fail-under = 85.0
verbose = true

[tool.tarpaulin.engines]
llvm = true
```

### Pre-commit Hook Installation

**Install pre-commit framework:**
```bash
# Install pre-commit
pip install pre-commit

# Install hooks
pre-commit install

# Install commit-msg hook for conventional commits
pre-commit install --hook-type commit-msg
```

## 📋 Development Workflow

### Before Every Commit

**REQUIRED CHECKS:**
```bash
# 1. Run tests with coverage
cargo tarpaulin --fail-under 85

# 2. Lint and format
cargo clippy -- -D warnings
cargo fmt --check

# 3. Security audit
cargo audit

# 4. Build verification
cargo build --all-features
cargo test --all-features

# 5. Documentation check
cargo doc --no-deps --all-features
```

### Pull Request Requirements

**MANDATORY CRITERIA:**
- [ ] All pre-commit hooks pass
- [ ] Code coverage meets thresholds
- [ ] No clippy warnings
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Security review completed (for crypto/handler changes)
- [ ] Performance impact assessed (for core changes)

**REVIEW PROCESS:**
- Minimum 1 reviewer for non-critical changes
- Minimum 2 reviewers for security-critical changes
- Maintainer approval required for architectural changes

### Branch Protection Rules

**Main Branch Protection:**
- Require pull request reviews
- Require status checks to pass
- Require branches to be up to date
- Require signed commits (recommended)
- No force pushes allowed
- No deletions allowed

## 🚨 Quality Gates

### Automated CI/CD Checks

**On Every Push:**
```yaml
# CI Pipeline Jobs
- code-coverage-check
- security-audit
- cross-platform-build
- integration-tests
- performance-benchmarks
- documentation-build
```

**Coverage Failure Actions:**
- Block merge if overall coverage < 85%
- Block merge if any file coverage < 80%
- Generate detailed coverage report
- Post coverage diff in PR comments

### Security Requirements

**MANDATORY SECURITY PRACTICES:**
- No hardcoded secrets or keys
- Constant-time cryptographic operations
- Input validation on all public APIs
- Error handling without information leakage
- Memory-safe operations (no unsafe blocks without justification)

**Security Review Triggers:**
- Changes to `src/crypto/`
- Changes to `src/handlers/`
- New dependencies
- Unsafe code additions
- External API integrations

## 📊 Metrics and Monitoring

### Coverage Reporting

**Report Generation:**
```bash
# Generate detailed coverage report
cargo tarpaulin --out Html --output-dir target/coverage

# View coverage by file
cargo tarpaulin --print-format=json | jq '.files'

# Continuous coverage tracking
cargo tarpaulin --coveralls <TOKEN>
```

**Coverage Badge Integration:**
```markdown
[![Coverage Status](https://coveralls.io/repos/github/username/stegano/badge.svg?branch=master)](https://coveralls.io/github/username/stegano?branch=master)
```

### Performance Monitoring

**Benchmark Requirements:**
- Performance regression checks
- Memory usage monitoring
- GPU acceleration verification
- Cryptographic operation timing

## 🔒 Enforcement Mechanisms

### Git Hooks Configuration

**Pre-commit Hook (`.git/hooks/pre-commit`):**
```bash
#!/bin/bash
set -e

echo "🔍 Running pre-commit checks..."

# Coverage check
echo "📊 Checking code coverage..."
cargo tarpaulin --skip-clean --fail-under 85 --quiet

# Linting
echo "🧹 Running cargo clippy..."
cargo clippy --all-targets --all-features -- -D warnings

# Formatting
echo "📝 Checking code formatting..."
cargo fmt --all -- --check

# Security audit
echo "🔒 Running security audit..."
cargo audit

echo "✅ All pre-commit checks passed!"
```

**Commit Message Hook (`.git/hooks/commit-msg`):**
```bash
#!/bin/bash
# Enforce conventional commit format
commit_regex='^(feat|fix|docs|style|refactor|test|chore|perf|ci|build|revert)(\(.+\))?: .{1,50}'

if ! grep -qE "$commit_regex" "$1"; then
    echo "❌ Commit message must follow conventional format:"
    echo "   type(scope): description"
    echo "   Example: feat(crypto): add ChaCha20 encryption support"
    exit 1
fi
```

### IDE Integration

**VS Code Settings (`.vscode/settings.json`):**
```json
{
    "rust-analyzer.check.command": "clippy",
    "rust-analyzer.check.extraArgs": ["--", "-D", "warnings"],
    "editor.formatOnSave": true,
    "files.trimTrailingWhitespace": true,
    "files.insertFinalNewline": true,
    "coverage-gutters.coverageFileNames": [
        "target/coverage/lcov.info"
    ]
}
```

## 📈 Continuous Improvement

### Weekly Quality Reviews

**QUALITY METRICS REVIEW:**
- Overall coverage trends
- Security vulnerability reports
- Performance regression analysis
- Code quality metrics
- Technical debt assessment

### Tool Updates

**MONTHLY MAINTENANCE:**
- Update linting tools
- Review coverage thresholds
- Assess new security tools
- Performance tooling updates

## 🚫 Non-Compliance Consequences

### Automated Actions

**COVERAGE FAILURES:**
- PR automatically blocked
- Coverage report posted to PR
- Developer notification sent
- Build status updated

**SECURITY VIOLATIONS:**
- Immediate build failure
- Security team notification
- Mandatory security review
- Escalation to maintainers

### Manual Interventions

**REPEATED VIOLATIONS:**
- Code review required for all changes
- Pair programming mandate
- Additional training assignment
- Escalation to project leadership

## ⚙️ Configuration Files

### `.pre-commit-config.yaml`
```yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.4.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
      - id: check-merge-conflict
      - id: check-added-large-files
        args: ['--maxkb=100000']
      - id: check-yaml
      - id: check-json
      - id: check-toml

  - repo: local
    hooks:
      - id: cargo-fmt
        name: cargo fmt
        entry: cargo fmt
        language: system
        types: [rust]
        args: ['--all', '--', '--check']
        
      - id: cargo-clippy
        name: cargo clippy
        entry: cargo clippy
        language: system
        types: [rust]
        args: ['--all-targets', '--all-features', '--', '-D', 'warnings']
        
      - id: cargo-test
        name: cargo test
        entry: cargo test
        language: system
        types: [rust]
        args: ['--all-features']
        
      - id: cargo-coverage
        name: cargo coverage
        entry: cargo tarpaulin
        language: system
        types: [rust]
        args: ['--skip-clean', '--fail-under', '85', '--quiet']
```

This development rules framework ensures consistent, high-quality code while maintaining security and performance standards throughout the project lifecycle.