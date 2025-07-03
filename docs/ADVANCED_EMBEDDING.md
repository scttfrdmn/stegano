# Advanced Format-Specific Embedding Strategies

This document describes the sophisticated steganography techniques implemented in the enhanced handlers for different data formats.

## Overview

The steganography system now supports multiple embedding strategies per format, allowing users to choose between different trade-offs of capacity, imperceptibility, and robustness. Strategy selection is automatic based on the configured stealth level.

## Signal/Audio Steganography Strategies

### 1. Time Domain LSB (Low Stealth)
- **Technique**: Direct manipulation of least significant bits in audio samples
- **Capacity**: ~1 bit per sample (highest capacity)
- **Robustness**: Low (vulnerable to compression/filtering)
- **Use Case**: Maximum data hiding when stealth is less critical

```rust
// 16-bit sample LSB modification
sample_int = (sample_int & 0xFFFE) | (bit as i16);
```

### 2. Frequency Domain FFT (Medium Stealth)
- **Technique**: Embedding in FFT magnitude coefficients of middle frequencies
- **Capacity**: ~8 bits per 1024-sample block in usable frequency bins
- **Robustness**: Medium (survives mild compression)
- **Use Case**: Balanced approach for general audio steganography

```rust
// Slight magnitude modification in frequency domain
let modifier = if bit == 1 { 1.001 } else { 0.999 };
buffer[bin] *= modifier;
```

### 3. Phase Modulation (High Stealth)
- **Technique**: Small phase shifts in FFT coefficients
- **Capacity**: ~2 bits per frequency bin (conservative)
- **Robustness**: High (phase changes are less perceptible)
- **Use Case**: High-security applications requiring imperceptibility

```rust
// Small phase shift based on data bit
let phase_shift = if bit == 1 { 0.1 } else { -0.1 };
buffer[bin] = Complex::from_polar(magnitude, current_phase + phase_shift);
```

### 4. Spread Spectrum (Maximum Stealth)
- **Technique**: Pseudorandom spreading across entire spectrum
- **Capacity**: ~100 bits per second (lowest capacity)
- **Robustness**: Very high (resistant to detection and attacks)
- **Use Case**: Covert communication requiring maximum stealth

```rust
// Low-amplitude pseudorandom spreading
*sample += amplitude * spread_bit * pn_sequence[i];
```

### 5. Echo Hiding
- **Technique**: Controlled delay-based echo addition
- **Capacity**: ~50 bits per second
- **Robustness**: High (echoes blend naturally with audio)
- **Use Case**: Audio streams where echo artifacts are acceptable

## Image Steganography Strategies

### 1. Simple LSB (Low Stealth)
- **Technique**: Sequential LSB replacement in image pixels
- **Capacity**: 3 bits per pixel (RGB channels)
- **Robustness**: Low (detectable with statistical analysis)
- **Use Case**: Maximum capacity for non-critical applications

```rust
// Direct LSB replacement
image_data[i] = (image_data[i] & 0xFE) | bit;
```

### 2. Adaptive LSB (Medium Stealth)
- **Technique**: LSB embedding prioritizing high-variation areas
- **Capacity**: ~2 bits per pixel (selective embedding)
- **Robustness**: Medium (better statistical properties)
- **Use Case**: General-purpose image steganography

```rust
// Embed only in high-variation regions
let variation = (pixel_diff_1).abs() + (pixel_diff_2).abs();
if variation > threshold {
    embed_bit();
}
```

### 3. Edge-Based Embedding (High/Maximum Stealth)
- **Technique**: Sobel-like edge detection for embedding site selection
- **Capacity**: ~30% of simple LSB (edge regions only)
- **Robustness**: Very high (changes masked by natural edges)
- **Use Case**: High-security image steganography

```rust
// Gradient-based edge detection
let gx = right_pixel - left_pixel;
let gy = bottom_pixel - top_pixel;
let gradient = sqrt(gx² + gy²);
if gradient > edge_threshold {
    embed_in_edge_region();
}
```

### 4. DCT Frequency Domain (Framework)
- **Technique**: Discrete Cosine Transform coefficient modification
- **Capacity**: ~12 coefficients per 8×8 block
- **Robustness**: High (JPEG-compatible)
- **Use Case**: Images that may undergo JPEG compression

### 5. Pixel Value Differencing (PVD)
- **Technique**: Embedding based on pixel pair differences
- **Capacity**: Variable based on pixel differences
- **Robustness**: High (preserves natural pixel relationships)
- **Use Case**: Natural images with varied content

## Stealth Level Mapping

| Stealth Level | Signal Strategy | Image Strategy | Trade-off |
|---------------|----------------|----------------|-----------|
| Low | Time Domain LSB | Simple LSB | Maximum capacity |
| Medium | Frequency Domain | Adaptive LSB | Balanced |
| High | Phase Modulation | Edge-Based | High imperceptibility |
| Maximum | Phase Modulation | Edge-Based | Maximum stealth |

## Format Detection

The system automatically detects file formats to select appropriate strategies:

### Audio Formats
- **WAV**: RIFF header detection (`RIFF...WAVE`)
- **FLAC**: fLaC magic bytes
- **MP3**: ID3v2 header detection

### Image Formats  
- **PNG**: PNG signature (89 50 4E 47...)
- **JPEG**: JPEG markers (FF D8 FF)
- **TIFF**: TIFF magic (II 2A 00 or MM 00 2A)
- **BMP**: BMP signature (42 4D)

## Capacity Estimation

Each strategy provides realistic capacity estimates:

```rust
// Example capacity calculation for frequency domain
let fft_size = 1024.min(sample_count);
let usable_bins = fft_size / 4; // Middle 25% of spectrum
let capacity_bits = usable_bins * 8; // 8 bits per bin
```

## Header Structure

All embedded data includes a structured header:

```
[4 bytes] Magic: "SGSJ" (signals) or "IMSJ" (images)
[4 bytes] Payload length (little-endian u32)
[1 byte]  Strategy identifier (0-4)
[1 byte]  Encryption flag (0/1)
[N bytes] Encrypted payload data
```

## Security Features

1. **Authenticated Encryption**: AES-256-GCM with BLAKE3 integrity hashing
2. **Payload Serialization**: Secure conversion between data types
3. **Strategy Obfuscation**: Strategy selection based on content analysis
4. **Anti-Detection**: Sophisticated algorithms that mimic natural variations

## Performance Characteristics

| Strategy | Embedding Speed | Extraction Speed | Memory Usage |
|----------|----------------|------------------|--------------|
| LSB | Very Fast | Very Fast | Low |
| FFT-based | Medium | Medium | Medium |
| Edge-based | Slow | Slow | High |
| Spread Spectrum | Fast | Fast | Low |

## Usage Examples

```rust
// Create handler with encryption
let handler = SignalHandler::with_encryption("secure_password")?;

// Configure parameters for high stealth
let mut params = EmbedParams::new(b"encryption_key".to_vec());
params.stealth_level = StealthLevel::High; // Uses phase modulation

// Embed with automatic strategy selection
let result = handler.embed(&mut carrier, payload, &params).await?;

// Extract with multi-strategy fallback
let extracted = handler.extract(&carrier, &params).await?;
```

This advanced embedding system provides researchers and security professionals with sophisticated tools for secure data hiding across multiple scientific and multimedia formats.