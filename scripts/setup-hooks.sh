#!/bin/bash
# Setup script for development environment and pre-commit hooks
# Ensures all required tools and configurations are in place

set -euo pipefail

echo "🔧 Setting up Stegano development environment..."

# Check if we're in the right directory
if [[ ! -f "Cargo.toml" ]]; then
    echo "❌ Error: Run this script from the project root directory"
    exit 1
fi

# Check for required system dependencies
check_dependency() {
    if ! command -v "$1" &> /dev/null; then
        echo "❌ Error: $1 is not installed"
        echo "   Please install $1 and try again"
        exit 1
    fi
}

echo "📋 Checking system dependencies..."
check_dependency "cargo"
check_dependency "git"
check_dependency "python3"
check_dependency "pip"

# Install Rust tools
echo "🦀 Installing Rust development tools..."
cargo install cargo-tarpaulin --locked || echo "⚠️  cargo-tarpaulin already installed"
cargo install cargo-audit --locked || echo "⚠️  cargo-audit already installed"
cargo install cargo-deny --locked || echo "⚠️  cargo-deny already installed"

# Install pre-commit
echo "🔗 Installing pre-commit framework..."
pip install pre-commit yamllint markdownlint-cli shellcheck-py detect-secrets ggshield || {
    echo "⚠️  Some tools may already be installed"
}

# Install pre-commit hooks
echo "🪝 Installing pre-commit hooks..."
pre-commit install
pre-commit install --hook-type commit-msg

# Generate initial secrets baseline
echo "🔒 Setting up secrets detection baseline..."
if [[ ! -f ".secrets.baseline" ]]; then
    detect-secrets scan --baseline .secrets.baseline || echo "⚠️  Secrets baseline already exists"
fi

# Create VS Code settings if not exists
if [[ ! -d ".vscode" ]]; then
    echo "💻 Setting up VS Code configuration..."
    mkdir -p .vscode
    cat > .vscode/settings.json << 'EOF'
{
    "rust-analyzer.check.command": "clippy",
    "rust-analyzer.check.extraArgs": ["--", "-D", "warnings"],
    "rust-analyzer.cargo.features": "all",
    "editor.formatOnSave": true,
    "files.trimTrailingWhitespace": true,
    "files.insertFinalNewline": true,
    "coverage-gutters.coverageFileNames": [
        "target/coverage/lcov.info"
    ],
    "coverage-gutters.showLineCoverage": true,
    "coverage-gutters.showRulerCoverage": true,
    "rust-analyzer.lens.enable": true,
    "rust-analyzer.lens.methodReferences": true,
    "rust-analyzer.lens.references": true
}
EOF
fi

# Create initial test coverage
echo "📊 Running initial coverage check..."
if cargo tarpaulin --skip-clean --timeout 300 --fail-under 0 > /dev/null 2>&1; then
    echo "✅ Initial coverage baseline established"
else
    echo "⚠️  Coverage check failed - this is expected for a new project"
fi

# Verify pre-commit installation
echo "🧪 Testing pre-commit configuration..."
if pre-commit run --all-files --show-diff-on-failure; then
    echo "✅ All pre-commit hooks pass"
else
    echo "⚠️  Some pre-commit hooks failed - this is expected initially"
    echo "   Run 'pre-commit run --all-files' to see details"
fi

# Create helpful aliases in git config
echo "⚙️  Setting up Git aliases..."
git config alias.coverage "!cargo tarpaulin --skip-clean"
git config alias.lint "!cargo clippy --all-targets --all-features -- -D warnings"
git config alias.fmt-check "!cargo fmt --all -- --check"
git config alias.security "!cargo audit"

echo ""
echo "🎉 Development environment setup complete!"
echo ""
echo "📝 Next steps:"
echo "   1. Run 'cargo test' to ensure tests pass"
echo "   2. Run 'cargo tarpaulin' to check current coverage"
echo "   3. Use 'git coverage' for quick coverage checks"
echo "   4. Commit changes to trigger pre-commit hooks"
echo ""
echo "🔧 Available Git aliases:"
echo "   git coverage  - Run coverage analysis"
echo "   git lint      - Run clippy linting"
echo "   git fmt-check - Check formatting"
echo "   git security  - Run security audit"
echo ""
echo "📊 Coverage requirements:"
echo "   • Overall project: ≥85%"
echo "   • Individual files: ≥80%"
echo "   • Critical modules: ≥95%"
echo ""
echo "🚨 Pre-commit hooks will enforce these standards automatically!"