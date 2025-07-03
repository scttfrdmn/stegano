#!/bin/bash
# Build script for stegano

echo "Building stegano..."

# Build release version
cargo build --release

# Build with GPU support
cargo build --release --features gpu

# Build CUDA kernels if available
if command -v nvcc &> /dev/null; then
    echo "Building CUDA kernels..."
    nvcc -c -o target/fft_embed.o gpu_kernels/cuda/fft_embed.cu
fi

echo "Build complete!"
