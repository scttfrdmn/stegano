#!/bin/bash
# Stegano Development Environment Setup Script
# This script sets up the development environment for stegano 0.2.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print functions
print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_header() {
    echo -e "${BLUE}"
    echo "=================================================="
    echo "  Stegano 0.2.0 Development Environment Setup"
    echo "=================================================="
    echo -e "${NC}"
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Install Rust if not present
install_rust() {
    if command_exists rustc; then
        print_success "Rust is already installed: $(rustc --version)"
    else
        print_info "Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source ~/.cargo/env
        print_success "Rust installed successfully"
    fi
    
    # Update Rust to latest stable
    print_info "Updating Rust to latest stable..."
    rustup update stable
    rustup default stable
    
    # Add required components
    print_info "Adding required Rust components..."
    rustup component add clippy rustfmt
    
    print_success "Rust toolchain configured"
}

# Install development tools
install_dev_tools() {
    print_info "Installing development tools..."
    
    # Essential cargo tools
    local tools=(
        "cargo-audit"      # Security auditing
        "cargo-tarpaulin"  # Code coverage
        "cargo-watch"      # File watching
        "cargo-expand"     # Macro expansion
        "cargo-criterion"  # Benchmarking
        "cargo-deny"       # License and security checking
        "tokei"           # Code metrics
    )
    
    for tool in "${tools[@]}"; do
        if cargo install --list | grep -q "^$tool "; then
            print_success "$tool is already installed"
        else
            print_info "Installing $tool..."
            cargo install "$tool"
            print_success "$tool installed"
        fi
    done
}

# Install system dependencies
install_system_deps() {
    print_info "Installing system dependencies..."
    
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux
        if command_exists apt-get; then
            # Ubuntu/Debian
            print_info "Installing dependencies for Ubuntu/Debian..."
            sudo apt-get update
            sudo apt-get install -y \
                pkg-config \
                libssl-dev \
                libudev-dev \
                mesa-vulkan-drivers \
                vulkan-tools \
                valgrind \
                clang \
                gcc \
                make
        elif command_exists yum; then
            # RHEL/CentOS
            print_info "Installing dependencies for RHEL/CentOS..."
            sudo yum install -y \
                pkg-config \
                openssl-devel \
                systemd-devel \
                vulkan-tools \
                valgrind \
                clang \
                gcc \
                make
        elif command_exists pacman; then
            # Arch Linux
            print_info "Installing dependencies for Arch Linux..."
            sudo pacman -S --noconfirm \
                pkg-config \
                openssl \
                systemd \
                vulkan-tools \
                valgrind \
                clang \
                gcc \
                make
        else
            print_warning "Unknown Linux distribution. Please install dependencies manually."
        fi
        
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        print_info "Installing dependencies for macOS..."
        if command_exists brew; then
            brew install pkg-config openssl
        else
            print_warning "Homebrew not found. Please install it first: https://brew.sh"
        fi
        
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        # Windows
        print_info "Windows detected. Please ensure you have:"
        print_info "  - Visual Studio Build Tools"
        print_info "  - Windows SDK"
        print_info "  - pkg-config (via vcpkg or msys2)"
        
    else
        print_warning "Unknown operating system: $OSTYPE"
    fi
    
    print_success "System dependencies configured"
}

# Install GPU development tools
install_gpu_tools() {
    print_info "Setting up GPU development environment..."
    
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Check for Vulkan support
        if command_exists vulkaninfo; then
            vulkaninfo --summary || print_warning "Vulkan not properly configured"
        else
            print_warning "Vulkan tools not found. GPU features may not work."
        fi
        
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS - Metal is built-in
        print_success "Metal support available on macOS"
        
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        # Windows
        print_info "Please ensure you have the latest GPU drivers installed"
    fi
}

# Setup Git configuration
setup_git() {
    print_info "Setting up Git configuration..."
    
    # Check if Git is configured
    if ! git config --global user.name >/dev/null 2>&1; then
        print_warning "Git user.name not configured. Please set it:"
        print_info "  git config --global user.name \"Your Name\""
    fi
    
    if ! git config --global user.email >/dev/null 2>&1; then
        print_warning "Git user.email not configured. Please set it:"
        print_info "  git config --global user.email \"your.email@example.com\""
    fi
    
    # Install Git hooks
    if [ -f "scripts/pre-commit" ]; then
        print_info "Installing Git pre-commit hook..."
        cp scripts/pre-commit .git/hooks/
        chmod +x .git/hooks/pre-commit
        print_success "Git pre-commit hook installed"
    fi
    
    # Configure Git for the project
    git config core.autocrlf input  # Handle line endings consistently
    git config pull.rebase false    # Use merge for pulls (not rebase)
    
    print_success "Git configuration complete"
}

# Setup IDE configuration
setup_ide() {
    print_info "Setting up IDE configuration..."
    
    # VS Code settings
    if [ -d ".vscode" ] || command_exists code; then
        print_info "VS Code detected. Recommended extensions:"
        print_info "  - rust-analyzer (Rust language support)"
        print_info "  - GitLens (Git integration)"
        print_info "  - Better TOML (Cargo.toml syntax)"
        print_info "  - Error Lens (Inline errors)"
        print_info "  - CodeLLDB (Debugging support)"
        
        # Create recommended extensions file
        mkdir -p .vscode
        cat > .vscode/extensions.json << EOF
{
    "recommendations": [
        "rust-lang.rust-analyzer",
        "eamodio.gitlens",
        "bungcip.better-toml",
        "usernamehw.errorlens",
        "vadimcn.vscode-lldb"
    ]
}
EOF
        print_success "VS Code extensions.json created"
    fi
    
    # Rust-analyzer configuration
    if [ ! -f "rust-project.json" ]; then
        print_info "Generating rust-project.json for better IDE support..."
        cargo metadata --format-version=1 > /dev/null 2>&1 || true
    fi
    
    print_success "IDE configuration complete"
}

# Setup development configuration
setup_dev_config() {
    print_info "Setting up development configuration..."
    
    # Create development configuration from template
    if [ -f "config/development.toml.example" ] && [ ! -f "config/development.toml" ]; then
        cp config/development.toml.example config/development.toml
        print_success "Development configuration created"
    fi
    
    # Create .env file for development
    if [ ! -f ".env" ]; then
        cat > .env << EOF
# Development environment variables
RUST_LOG=debug
RUST_BACKTRACE=1
STEGANO_LOG_LEVEL=debug
STEGANO_GPU_ENABLED=true
EOF
        print_success ".env file created"
    fi
    
    # Setup test data directory
    mkdir -p tests/data
    mkdir -p examples/data
    
    print_success "Development configuration complete"
}

# Verify installation
verify_installation() {
    print_info "Verifying installation..."
    
    # Check Rust
    if command_exists cargo; then
        cargo --version
        rustc --version
    else
        print_error "Cargo not found in PATH"
        return 1
    fi
    
    # Check project builds
    print_info "Testing project build..."
    if cargo check --all-features; then
        print_success "Project builds successfully"
    else
        print_error "Project build failed"
        return 1
    fi
    
    # Check tests
    print_info "Running quick test suite..."
    if cargo test --lib --quiet; then
        print_success "Core tests pass"
    else
        print_warning "Some tests failed - this may be expected during development"
    fi
    
    # Check formatting and linting
    print_info "Checking code formatting..."
    if cargo fmt -- --check >/dev/null 2>&1; then
        print_success "Code formatting is correct"
    else
        print_warning "Code formatting issues found. Run 'cargo fmt' to fix."
    fi
    
    print_info "Running clippy..."
    if cargo clippy --all-features --quiet >/dev/null 2>&1; then
        print_success "No clippy warnings"
    else
        print_warning "Clippy warnings found. Run 'cargo clippy' to see details."
    fi
    
    print_success "Installation verification complete"
}

# Print usage information
print_usage() {
    print_info "Development environment is ready! 🚀"
    echo ""
    print_info "Common development commands:"
    echo "  cargo run --example basic_demo        # Run basic example"
    echo "  cargo test                           # Run test suite"
    echo "  cargo test --all-features            # Run all tests"
    echo "  cargo bench                          # Run benchmarks"
    echo "  cargo watch -x test                  # Watch for changes and test"
    echo "  cargo clippy --all-features          # Run linter"
    echo "  cargo fmt                            # Format code"
    echo "  cargo audit                          # Security audit"
    echo "  cargo tarpaulin --all-features       # Generate coverage report"
    echo ""
    print_info "Branch workflow:"
    echo "  git checkout develop                 # Switch to develop branch"
    echo "  git checkout -b feature/my-feature   # Create feature branch"
    echo "  # ... make changes ..."
    echo "  git add . && git commit              # Commit changes"
    echo "  git push -u origin feature/my-feature # Push feature branch"
    echo "  gh pr create --base develop          # Create pull request"
    echo ""
    print_info "Documentation:"
    echo "  docs/README.md                       # Documentation index"
    echo "  docs/guides/getting-started.md       # Getting started guide"
    echo "  DEVELOPMENT_STRATEGY.md              # Development workflow"
    echo "  ROADMAP_0_2_0.md                     # Implementation roadmap"
}

# Main execution
main() {
    print_header
    
    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ] || ! grep -q "name = \"stegano\"" Cargo.toml; then
        print_error "This script must be run from the stegano project root directory"
        exit 1
    fi
    
    # Parse command line arguments
    SKIP_DEPS=false
    SKIP_GPU=false
    QUIET=false
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-deps)
                SKIP_DEPS=true
                shift
                ;;
            --skip-gpu)
                SKIP_GPU=true
                shift
                ;;
            --quiet)
                QUIET=true
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo "Options:"
                echo "  --skip-deps    Skip system dependency installation"
                echo "  --skip-gpu     Skip GPU tool setup"
                echo "  --quiet        Minimize output"
                echo "  -h, --help     Show this help"
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Run setup steps
    install_rust
    install_dev_tools
    
    if [ "$SKIP_DEPS" = false ]; then
        install_system_deps
    else
        print_warning "Skipping system dependency installation"
    fi
    
    if [ "$SKIP_GPU" = false ]; then
        install_gpu_tools
    else
        print_warning "Skipping GPU tool setup"
    fi
    
    setup_git
    setup_ide
    setup_dev_config
    verify_installation
    
    if [ "$QUIET" = false ]; then
        print_usage
    fi
    
    print_success "Development environment setup complete! 🎉"
}

# Run main function
main "$@"