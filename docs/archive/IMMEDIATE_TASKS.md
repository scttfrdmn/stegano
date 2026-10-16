# Immediate Tasks for 0.1.0 Release

## 🚨 **CRITICAL - Day 1 Tasks**

### **Fix Test Failures (4-6 hours)**

#### **1. Imagery Handler Index Bounds (High Priority)**
**File**: `src/handlers/imagery.rs:262`
**Error**: `index out of bounds: the len is 15000 but the index is 15000`

```rust
// CURRENT ISSUE: Line ~262
for i in 0..total_bits_needed {
    if i >= carrier.raw_data.len() {  // ❌ Wrong comparison
        break;
    }
    all_bits.push(carrier.raw_data[i] & 1);
}

// SOLUTION: Fix bounds checking
for i in 0..total_bits_needed.min(carrier.raw_data.len()) {
    all_bits.push(carrier.raw_data[i] & 1);
}
```

#### **2. Signals Handler Extraction Logic (High Priority)**
**File**: `src/handlers/signals.rs:649`
**Error**: `InvalidParameter("No valid extraction strategy found")`

```rust
// ISSUE: Header magic detection failing
// Root cause: Bit ordering in header embedding vs extraction

// DEBUG STEPS:
1. Print actual header bits being embedded
2. Print extracted header bits  
3. Compare bit-to-byte conversion
4. Fix endianness issues if found
```

#### **3. Capacity Estimation Bugs (Medium Priority)**
**File**: `src/handlers/signals.rs:669`  
**Error**: `InsufficientCapacity { needed: 94, available: 0 }`

```rust
// ISSUE: Capacity calculation returning 0
// Check in estimate_signal_capacity function

// DEBUG STEPS:
1. Print capacity_bits and overhead values
2. Verify sample_count calculation  
3. Check recommended_bits calculation
4. Ensure positive capacity values
```

### **Implementation Plan - Day 1**

```bash
# Hour 1-2: Debug and fix imagery handler
git checkout -b fix/imagery-bounds-checking
# Fix index bounds issues in imagery extraction
cargo test handlers::imagery::tests::test_lsb_embedding_roundtrip
cargo test handlers::imagery::tests::test_encrypted_embedding_roundtrip

# Hour 3-4: Debug and fix signals handler extraction
git checkout -b fix/signals-extraction  
# Add debug prints to understand header extraction failure
cargo test handlers::signals::tests::test_time_domain_embedding_roundtrip

# Hour 5-6: Fix capacity estimation
# Debug capacity calculation logic
cargo test handlers::signals::tests::test_encrypted_signal_embedding

# Verify all fixes
cargo test --lib
```

---

## ⚡ **URGENT - Day 2 Tasks**

### **1. Basic CLI Implementation (4-6 hours)**

#### **Create CLI Structure**
```bash
# Create new CLI module
mkdir src/cli
touch src/cli/mod.rs src/cli/commands.rs src/cli/args.rs
```

```rust
// src/cli/mod.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "stegano")]
#[command(about = "Scientific data steganography tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Embed secret data into carrier file
    Embed {
        /// Input carrier file
        #[arg(short, long)]
        input: String,
        /// Secret payload file
        #[arg(short, long)]
        payload: String,
        /// Output file with embedded data
        #[arg(short, long)]
        output: String,
        /// Encryption password
        #[arg(long)]
        password: Option<String>,
    },
    /// Extract secret data from carrier file
    Extract {
        /// Input file with embedded data
        #[arg(short, long)]
        input: String,
        /// Output file for extracted data
        #[arg(short, long)]
        output: String,
        /// Decryption password
        #[arg(long)]
        password: Option<String>,
    },
    /// Analyze carrier capacity
    Analyze {
        /// Input carrier file
        #[arg(short, long)]
        input: String,
    },
}
```

```rust
// src/bin/stegano.rs
use clap::Parser;
use stegano::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Embed { input, payload, output, password } => {
            // Implementation
        }
        Commands::Extract { input, output, password } => {
            // Implementation  
        }
        Commands::Analyze { input } => {
            // Implementation
        }
    }
    
    Ok(())
}
```

### **2. Clean Compilation Warnings (1 hour)**

```bash
# Fix unused imports and variables
cargo fix --lib --allow-dirty --allow-staged

# Address clippy warnings  
cargo clippy --fix --lib --allow-dirty --allow-staged

# Format code
cargo fmt

# Verify clean compilation
cargo check
```

---

## 📋 **Week 1 Checklist**

### **Day 1: Critical Fixes**
- [ ] Fix imagery handler index bounds errors
- [ ] Fix signals handler extraction logic  
- [ ] Resolve capacity estimation bugs
- [ ] Achieve 100% test pass rate
- [ ] Zero compiler warnings

### **Day 2: CLI Foundation** 
- [ ] Implement basic CLI with clap
- [ ] Add embed/extract/analyze subcommands
- [ ] Test CLI with sample files
- [ ] Add progress indicators
- [ ] Basic error handling

### **Day 3: Testing & Validation**
- [ ] Create comprehensive test data sets
- [ ] Test all format handlers via CLI
- [ ] Validate roundtrip integrity  
- [ ] Performance baseline measurement
- [ ] Memory usage profiling

### **Day 4: Documentation**
- [ ] Update README with CLI usage
- [ ] Add rustdoc to public APIs
- [ ] Create usage examples
- [ ] Update CHANGELOG for 0.1.0
- [ ] Version bump and release notes

### **Day 5: Release Preparation**
- [ ] Final testing on clean environment
- [ ] Security audit with cargo-audit
- [ ] Package metadata verification  
- [ ] Git tag and release preparation
- [ ] Deployment documentation

---

## 🔧 **Quick Diagnostic Commands**

### **Identify Current Issues**
```bash
# Test status
cargo test --lib 2>&1 | grep "test result:"

# Specific failing tests
cargo test --lib handlers::imagery::tests::test_lsb_embedding_roundtrip -- --nocapture
cargo test --lib handlers::signals::tests::test_time_domain_embedding_roundtrip -- --nocapture

# Compilation warnings
cargo check 2>&1 | grep "warning:"

# Code coverage
cargo tarpaulin --skip-clean --ignore-tests
```

### **Performance Baseline**
```bash
# Current performance
cargo bench --bench embedding_performance

# Memory usage
cargo run --example memory_usage_test

# Binary size
cargo build --release && ls -lh target/release/stegano
```

---

## 📊 **Success Criteria Verification**

### **Before 0.1.0 Release**
```bash
#!/bin/bash
# verify_release_ready.sh

echo "🔍 Verifying 0.1.0 release readiness..."

# Test suite
echo "Running test suite..."
cargo test --all-features --lib
if [ $? -ne 0 ]; then echo "❌ Tests failing"; exit 1; fi

# Compilation clean
echo "Checking compilation warnings..."
cargo check --all-targets 2>&1 | grep -q "warning:"
if [ $? -eq 0 ]; then echo "❌ Compilation warnings present"; exit 1; fi

# CLI functionality
echo "Testing CLI..."
cargo run --bin stegano -- --help > /dev/null
if [ $? -ne 0 ]; then echo "❌ CLI not working"; exit 1; fi

# Documentation builds
echo "Building documentation..."
cargo doc --all-features --no-deps > /dev/null
if [ $? -ne 0 ]; then echo "❌ Documentation build failed"; exit 1; fi

# Security audit
echo "Running security audit..."
cargo audit > /dev/null
if [ $? -ne 0 ]; then echo "❌ Security vulnerabilities found"; exit 1; fi

echo "✅ Release readiness verified!"
```

---

## 💡 **Implementation Tips**

### **Debugging Test Failures**
```rust
// Add debug output to understand failures
#[cfg(test)]
mod debug_tests {
    #[test]
    fn debug_extraction() {
        let handler = SignalHandler::new();
        // ... setup code ...
        
        // Debug prints
        println!("Header bits: {:?}", &header_bits[0..32]);
        println!("Header bytes: {:?}", &header_bytes[0..4]);
        println!("Expected: {:?}", b"SGSJ");
        
        // Test extraction step by step
        assert_eq!(&header_bytes[0..4], b"SGSJ");
    }
}
```

### **CLI Error Handling**
```rust
// Proper error handling in CLI
match handler.embed(&mut carrier, &payload, &params).await {
    Ok(result) => {
        println!("✅ Successfully embedded {} bytes", result.bytes_embedded);
        println!("📊 Performance: {:.2}ms", result.performance_stats.total_time_ms());
    }
    Err(e) => {
        eprintln!("❌ Embedding failed: {}", e);
        std::process::exit(1);
    }
}
```

### **Progress Indicators**
```rust
// Simple progress bar
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total_bytes as u64);
pb.set_style(
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .progress_chars("##-")
);

// Update progress during processing
pb.set_message("Embedding data...");
pb.inc(bytes_processed as u64);
```

---

## 🎯 **Outcome of This Phase**

After completing these immediate tasks, you'll have:

1. **Stable Foundation**: All tests passing, zero warnings
2. **Usable CLI Tool**: Professional command-line interface  
3. **Clear Documentation**: API docs and usage examples
4. **Release Readiness**: 0.1.0 ready for deployment
5. **Performance Baseline**: Documented performance characteristics

This creates a solid foundation for:
- **Enterprise pilot programs**
- **Technical demonstrations**  
- **Partnership discussions**
- **Investment presentations**
- **Open source community building**

The 0.1.0 release will demonstrate the full potential of your steganography platform while providing a professional, stable foundation for rapid commercial development.