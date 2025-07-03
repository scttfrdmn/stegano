#!/bin/bash
# Test script for stegano

echo "Running tests..."

# Unit tests
cargo test --lib

# Integration tests  
cargo test --test integration

# Benchmarks
cargo bench --features benchmarks

# GPU tests (if available)
if command -v nvidia-smi &> /dev/null; then
    cargo test --features gpu test_gpu
fi

echo "Tests complete!"
