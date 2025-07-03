// CUDA kernel for FFT-based embedding in frequency domain
#include <cuda_runtime.h>
#include <cufft.h>

extern "C" {

__global__ void embed_in_fft_coefficients(
    cufftComplex* fft_data,
    const unsigned char* payload_bits,
    int num_coefficients,
    int payload_length,
    float embed_strength
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    
    if (idx >= num_coefficients || idx >= payload_length) {
        return;
    }
    
    // Extract bit from payload
    int byte_idx = idx / 8;
    int bit_idx = idx % 8;
    int bit = (payload_bits[byte_idx] >> bit_idx) & 1;
    
    // Modify magnitude while preserving phase
    float magnitude = sqrtf(fft_data[idx].x * fft_data[idx].x + fft_data[idx].y * fft_data[idx].y);
    float phase = atan2f(fft_data[idx].y, fft_data[idx].x);
    
    float new_magnitude = magnitude * (1.0f + (bit ? embed_strength : -embed_strength));
    
    fft_data[idx].x = new_magnitude * cosf(phase);
    fft_data[idx].y = new_magnitude * sinf(phase);
}

}
