#!/bin/bash
# Security and supply chain validation script
set -euo pipefail

echo "🔒 Stegano Security & Supply Chain Audit"
echo "========================================"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Install required tools if not present
install_tools() {
    echo "📦 Installing security tools..."
    
    if ! command -v cargo-audit &> /dev/null; then
        echo "Installing cargo-audit..."
        cargo install cargo-audit
    fi
    
    if ! command -v cargo-deny &> /dev/null; then
        echo "Installing cargo-deny..."
        cargo install cargo-deny
    fi
    
    if ! command -v cargo-outdated &> /dev/null; then
        echo "Installing cargo-outdated..."
        cargo install cargo-outdated
    fi
    
    print_status "Security tools installed"
}

# Run vulnerability audit
run_audit() {
    echo
    echo "🔍 Running vulnerability audit..."
    
    if cargo audit; then
        print_status "No known vulnerabilities found"
    else
        print_error "Vulnerabilities detected! Check output above."
        return 1
    fi
}

# Run cargo-deny checks
run_deny() {
    echo
    echo "🚫 Running cargo-deny checks..."
    
    if [ -f "deny.toml" ]; then
        if cargo deny check; then
            print_status "All cargo-deny checks passed"
        else
            print_error "cargo-deny checks failed! Check output above."
            return 1
        fi
    else
        print_warning "deny.toml not found, skipping cargo-deny checks"
    fi
}

# Check for outdated dependencies
check_outdated() {
    echo
    echo "📅 Checking for outdated dependencies..."
    
    echo "Outdated dependencies:"
    cargo outdated || print_warning "cargo-outdated check failed"
}

# Generate dependency tree
dependency_tree() {
    echo
    echo "🌳 Dependency tree analysis..."
    
    echo "Direct dependencies:"
    cargo tree --depth 1
    
    echo
    echo "Total dependency count:"
    cargo tree --depth 99 | wc -l
}

# License compliance check
license_check() {
    echo
    echo "📄 License compliance check..."
    
    if [ -f "deny.toml" ]; then
        if cargo deny check licenses; then
            print_status "License compliance verified"
        else
            print_error "License compliance issues found!"
            return 1
        fi
    else
        print_warning "deny.toml not found, manual license review required"
    fi
}

# Generate security report
generate_report() {
    echo
    echo "📊 Generating security report..."
    
    REPORT_FILE="security_report_$(date +%Y%m%d_%H%M%S).txt"
    
    {
        echo "Stegano Security & Supply Chain Report"
        echo "====================================="
        echo "Generated: $(date)"
        echo "Git commit: $(git rev-parse HEAD 2>/dev/null || echo 'Unknown')"
        echo
        
        echo "=== Cargo Audit Results ==="
        cargo audit --json 2>/dev/null | jq '.' 2>/dev/null || cargo audit
        echo
        
        echo "=== Dependency Tree ==="
        cargo tree --depth 3
        echo
        
        echo "=== License Summary ==="
        cargo deny check licenses 2>&1 || echo "License check failed"
        echo
        
        echo "=== Outdated Dependencies ==="
        cargo outdated 2>&1 || echo "Outdated check failed"
        
    } > "$REPORT_FILE"
    
    print_status "Security report saved to: $REPORT_FILE"
}

# Main execution
main() {
    echo "Starting security audit for $(pwd)"
    echo
    
    install_tools
    
    local exit_code=0
    
    run_audit || exit_code=1
    run_deny || exit_code=1
    license_check || exit_code=1
    check_outdated
    dependency_tree
    generate_report
    
    echo
    if [ $exit_code -eq 0 ]; then
        print_status "Security audit completed successfully!"
    else
        print_error "Security audit completed with issues. Please review the output above."
    fi
    
    return $exit_code
}

# Run main function
main "$@"