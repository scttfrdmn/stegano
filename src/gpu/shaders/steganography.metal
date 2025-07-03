//
// Metal compute shaders for steganography operations
// Optimized for Apple Silicon and Intel-based Macs
//

#include <metal_stdlib>
using namespace metal;

// Forward declarations of utility functions
uint8_t gf_multiply_func(uint8_t a, uint8_t b);
uint32_t reverse_bits_func(uint32_t x);

// LSB Embedding Kernel
// Embeds payload bits into the least significant bits of carrier data
kernel void lsb_embed_kernel(device uint8_t* carrier [[buffer(0)]],
                            const device uint8_t* payload_bits [[buffer(1)]],
                            const device uint32_t& payload_length [[buffer(2)]],
                            uint tid [[thread_position_in_grid]]) {
    
    if (tid >= payload_length) return;
    
    // Clear LSB and set with payload bit
    carrier[tid + 32] = (carrier[tid + 32] & 0xFE) | payload_bits[tid];
}

// LSB Extraction Kernel  
// Extracts payload bits from the least significant bits of carrier data
kernel void lsb_extract_kernel(const device uint8_t* carrier [[buffer(0)]],
                              device uint8_t* payload_bits [[buffer(1)]],
                              const device uint32_t& payload_length [[buffer(2)]],
                              uint tid [[thread_position_in_grid]]) {
    
    if (tid >= payload_length) return;
    
    // Extract LSB
    payload_bits[tid] = carrier[tid + 32] & 1;
}

// Simple FFT implementation for small signals
// Note: For production use, consider using Accelerate framework's vDSP
kernel void fft_kernel(const device float* input [[buffer(0)]],
                      device float* output [[buffer(1)]],
                      const device uint32_t& length [[buffer(2)]],
                      uint tid [[thread_position_in_grid]]) {
    
    if (tid >= length) return;
    
    // Simple DFT implementation (O(N²) - suitable for small signals)
    float real_sum = 0.0;
    float imag_sum = 0.0;
    
    for (uint k = 0; k < length; k++) {
        float angle = -2.0 * M_PI_F * tid * k / length;
        real_sum += input[k] * cos(angle);
        imag_sum += input[k] * sin(angle);
    }
    
    output[tid * 2] = real_sum;      // Real part
    output[tid * 2 + 1] = imag_sum;  // Imaginary part
}

// Optimized FFT using Cooley-Tukey algorithm (radix-2)
kernel void fft_radix2_kernel(device float2* data [[buffer(0)]],
                             const device uint32_t& length [[buffer(1)]],
                             const device uint32_t& stage [[buffer(2)]],
                             uint tid [[thread_position_in_grid]]) {
    
    uint step = 1 << stage;
    uint pair_dist = step >> 1;
    uint group_size = step;
    
    if (tid >= length / 2) return;
    
    uint group = tid / pair_dist;
    uint pos_in_group = tid % pair_dist;
    uint i = group * group_size + pos_in_group;
    uint j = i + pair_dist;
    
    if (j >= length) return;
    
    // Twiddle factor
    float angle = -2.0 * M_PI_F * pos_in_group / step;
    float2 twiddle = float2(cos(angle), sin(angle));
    
    // Complex multiplication: data[j] *= twiddle
    float2 temp = data[j];
    data[j] = float2(
        temp.x * twiddle.x - temp.y * twiddle.y,
        temp.x * twiddle.y + temp.y * twiddle.x
    );
    
    // Butterfly operation
    float2 u = data[i];
    float2 v = data[j];
    data[i] = u + v;
    data[j] = u - v;
}

// Reed-Solomon encoding kernel (simplified Galois field operations)
kernel void reed_solomon_kernel(const device uint8_t* input [[buffer(0)]],
                               device uint8_t* output [[buffer(1)]],
                               const device uint32_t& data_length [[buffer(2)]],
                               const device uint32_t& parity_bytes [[buffer(3)]],
                               uint tid [[thread_position_in_grid]]) {
    
    if (tid >= data_length + parity_bytes) return;
    
    // Copy input data first
    if (tid < data_length) {
        output[tid] = input[tid];
        return;
    }
    
    // Calculate parity bytes (simplified implementation)
    uint parity_idx = tid - data_length;
    uint8_t parity = 0;
    
    // Simple XOR-based parity (in production, use proper GF(256) arithmetic)
    for (uint i = 0; i < data_length; i++) {
        uint8_t coeff = ((parity_idx + 1) * (i + 1)) & 0xFF;
        parity ^= gf_multiply_func(input[i], coeff);
    }
    
    output[tid] = parity;
}

// Galois Field multiplication in GF(256)
uint8_t gf_multiply_func(uint8_t a, uint8_t b) {
    uint8_t result = 0;
    uint8_t hi_bit_set;
    
    for (int i = 0; i < 8; i++) {
        if (b & 1) {
            result ^= a;
        }
        hi_bit_set = a & 0x80;
        a <<= 1;
        if (hi_bit_set) {
            a ^= 0x1D; // Irreducible polynomial for AES GF(256)
        }
        b >>= 1;
    }
    
    return result;
}

// Parallel image processing kernel for spatial domain steganography
kernel void spatial_embed_kernel(device uint8_t* image [[buffer(0)]],
                                const device uint8_t* payload [[buffer(1)]],
                                const device uint32_t& width [[buffer(2)]],
                                const device uint32_t& height [[buffer(3)]],
                                const device uint32_t& channels [[buffer(4)]],
                                const device uint32_t& payload_bits [[buffer(5)]],
                                uint2 tid [[thread_position_in_grid]]) {
    
    if (tid.x >= width || tid.y >= height) return;
    
    uint pixel_idx = (tid.y * width + tid.x) * channels;
    uint bit_idx = tid.y * width + tid.x;
    
    if (bit_idx >= payload_bits) return;
    
    // Embed in the blue channel's LSB (typically least noticeable)
    uint8_t payload_bit = payload[bit_idx / 8] >> (bit_idx % 8) & 1;
    image[pixel_idx + 2] = (image[pixel_idx + 2] & 0xFE) | payload_bit;
}

// Frequency domain steganography using DCT coefficients
kernel void dct_embed_kernel(device float* coefficients [[buffer(0)]],
                            const device uint8_t* payload [[buffer(1)]],
                            const device uint32_t& block_size [[buffer(2)]],
                            const device uint32_t& payload_bits [[buffer(3)]],
                            uint2 tid [[thread_position_in_grid]]) {
    
    if (tid.x >= block_size || tid.y >= block_size) return;
    
    uint coeff_idx = tid.y * block_size + tid.x;
    
    // Skip DC coefficient and very low frequency components
    if (coeff_idx < 3 || coeff_idx >= payload_bits + 3) return;
    
    uint bit_idx = coeff_idx - 3;
    uint8_t payload_bit = payload[bit_idx / 8] >> (bit_idx % 8) & 1;
    
    // Quantization-based embedding
    float quantization_step = 2.0;
    float quantized = round(coefficients[coeff_idx] / quantization_step);
    
    // Make quantized value odd/even based on payload bit
    if ((int(quantized) & 1) != payload_bit) {
        quantized += (quantized > 0) ? 1 : -1;
    }
    
    coefficients[coeff_idx] = quantized * quantization_step;
}

// High-performance parallel bit manipulation
kernel void parallel_bit_operations(device uint32_t* data [[buffer(0)]],
                                   const device uint32_t& operation [[buffer(1)]],
                                   const device uint32_t& length [[buffer(2)]],
                                   uint tid [[thread_position_in_grid]]) {
    
    if (tid >= length) return;
    
    switch (operation) {
        case 0: // Bit reversal
            data[tid] = reverse_bits_func(data[tid]);
            break;
        case 1: // Population count
            data[tid] = popcount(data[tid]);
            break;
        case 2: // Hamming weight
            data[tid] = popcount(data[tid] ^ ((tid > 0) ? data[tid - 1] : 0));
            break;
    }
}

// Utility function to reverse bits in a 32-bit integer
uint32_t reverse_bits_func(uint32_t x) {
    x = ((x & 0xAAAAAAAA) >> 1) | ((x & 0x55555555) << 1);
    x = ((x & 0xCCCCCCCC) >> 2) | ((x & 0x33333333) << 2);
    x = ((x & 0xF0F0F0F0) >> 4) | ((x & 0x0F0F0F0F) << 4);
    x = ((x & 0xFF00FF00) >> 8) | ((x & 0x00FF00FF) << 8);
    return (x >> 16) | (x << 16);
}

// Advanced steganography with adaptive embedding
kernel void adaptive_embed_kernel(device uint8_t* carrier [[buffer(0)]],
                                 const device uint8_t* payload [[buffer(1)]],
                                 device float* complexity_map [[buffer(2)]],
                                 const device uint32_t& width [[buffer(3)]],
                                 const device uint32_t& height [[buffer(4)]],
                                 const device float& threshold [[buffer(5)]],
                                 uint2 tid [[thread_position_in_grid]]) {
    
    if (tid.x >= width || tid.y >= height) return;
    
    uint idx = tid.y * width + tid.x;
    
    // Only embed in regions with sufficient complexity
    if (complexity_map[idx] > threshold) {
        uint bit_idx = idx;
        uint8_t payload_bit = payload[bit_idx / 8] >> (bit_idx % 8) & 1;
        carrier[idx] = (carrier[idx] & 0xFE) | payload_bit;
    }
}