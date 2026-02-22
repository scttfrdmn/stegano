//! CUDA acceleration for stegano
//!
//! Provides GPU-accelerated LSB embedding and extraction using NVIDIA CUDA.
//! Kernels are compiled at runtime via NVRTC so forward compatibility with
//! future GPU architectures (e.g. Blackwell sm_120) is handled by the driver.
//!
//! This module is only compiled when the `cuda` feature is enabled.

#[cfg(feature = "cuda")]
mod inner {
    use crate::core::*;
    use cudarc::driver::{CudaDevice, LaunchAsync, LaunchConfig};
    use cudarc::nvrtc::compile_ptx;
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Kernel source code
    // -----------------------------------------------------------------------

    /// LSB embedding: writes each payload bit into the LSB of the corresponding
    /// cover byte.  Both buffers have `n` elements.
    const LSB_EMBED_SRC: &str = r#"
extern "C" __global__ void lsb_embed(
    unsigned char* __restrict__ data,
    const unsigned char* __restrict__ payload,
    unsigned int n
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        data[i] = (data[i] & 0xFEU) | (payload[i] & 0x01U);
    }
}
"#;

    /// LSB extraction: reads the LSB of each cover byte into the output buffer.
    const LSB_EXTRACT_SRC: &str = r#"
extern "C" __global__ void lsb_extract(
    const unsigned char* __restrict__ data,
    unsigned char* __restrict__ output,
    unsigned int n
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        output[i] = data[i] & 0x01U;
    }
}
"#;

    // -----------------------------------------------------------------------
    // ML kernel source code
    // -----------------------------------------------------------------------

    /// Dense layer forward pass with sigmoid activation.
    /// out[i] = sigmoid( sum_j( w[i*in_size + j] * x[j] ) + b[i] )
    const DENSE_SIGMOID_SRC: &str = r#"
extern "C" __global__ void dense_sigmoid(
    const float* __restrict__ w,
    const float* __restrict__ x,
    const float* __restrict__ b,
    float* __restrict__ out,
    unsigned int in_size,
    unsigned int out_size
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < out_size) {
        float acc = b[i];
        for (unsigned int j = 0; j < in_size; j++) {
            acc += w[i * in_size + j] * x[j];
        }
        out[i] = 1.0f / (1.0f + expf(-acc));
    }
}
"#;

    /// SGD weight update: params[i] -= lr * grads[i]
    const SGD_UPDATE_SRC: &str = r#"
extern "C" __global__ void sgd_update(
    float* __restrict__ params,
    const float* __restrict__ grads,
    float lr,
    unsigned int n
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        params[i] -= lr * grads[i];
    }
}
"#;

    /// Output layer delta: delta[i] = (out[i] - target[i]) * out[i] * (1 - out[i])
    /// (MSE gradient combined with sigmoid derivative)
    const OUTPUT_DELTA_SRC: &str = r#"
extern "C" __global__ void output_delta(
    const float* __restrict__ output,
    const float* __restrict__ target,
    float* __restrict__ delta,
    unsigned int n
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        float s = output[i];
        delta[i] = (s - target[i]) * s * (1.0f - s);
    }
}
"#;

    /// Weight gradient (outer product): grad[i*in + j] = delta[i] * input[j]
    const WEIGHT_GRAD_SRC: &str = r#"
extern "C" __global__ void weight_grad(
    const float* __restrict__ delta,
    const float* __restrict__ input,
    float* __restrict__ grad,
    unsigned int in_size,
    unsigned int out_size
) {
    unsigned int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < out_size * in_size) {
        unsigned int i = idx / in_size;
        unsigned int j = idx % in_size;
        grad[idx] = delta[i] * input[j];
    }
}
"#;

    /// Hidden layer delta for backprop through a sigmoid layer:
    /// h_delta[j] = hidden[j]*(1-hidden[j]) * sum_i( w[i*hidden_size+j] * out_delta[i] )
    const HIDDEN_DELTA_SRC: &str = r#"
extern "C" __global__ void hidden_delta(
    const float* __restrict__ w,
    const float* __restrict__ out_delta,
    const float* __restrict__ hidden,
    float* __restrict__ h_delta,
    unsigned int hidden_size,
    unsigned int out_size
) {
    unsigned int j = blockIdx.x * blockDim.x + threadIdx.x;
    if (j < hidden_size) {
        float acc = 0.0f;
        for (unsigned int i = 0; i < out_size; i++) {
            acc += w[i * hidden_size + j] * out_delta[i];
        }
        float h = hidden[j];
        h_delta[j] = acc * h * (1.0f - h);
    }
}
"#;

    /// Byte histogram: atomicAdd each byte value into a 256-bucket histogram.
    const BYTE_HISTOGRAM_SRC: &str = r#"
extern "C" __global__ void byte_histogram(
    const unsigned char* __restrict__ data,
    unsigned int* __restrict__ hist,
    unsigned int n
) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        atomicAdd(&hist[(unsigned int)data[i]], 1u);
    }
}
"#;

    // -----------------------------------------------------------------------
    // CudaAccelerator
    // -----------------------------------------------------------------------

    /// CUDA-backed accelerator for steganographic LSB and ML operations.
    pub struct CudaAccelerator {
        device: Arc<CudaDevice>,
    }

    impl std::fmt::Debug for CudaAccelerator {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let name = self.device.name().unwrap_or_else(|_| "unknown".into());
            f.debug_struct("CudaAccelerator")
                .field("device", &name)
                .finish()
        }
    }

    impl CudaAccelerator {
        /// Returns `true` when at least one CUDA-capable device is present.
        pub fn is_available() -> bool {
            CudaDevice::new(0).is_ok()
        }

        /// Initialise the accelerator on device 0.  Compiles both kernels via
        /// NVRTC so the driver can JIT them to the native architecture.
        ///
        /// `CudaDevice::new` returns `Arc<CudaDevice>` directly in cudarc 0.12.
        pub fn new() -> Result<Self> {
            let device: Arc<CudaDevice> = CudaDevice::new(0)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Compile and load LSB embed kernel
            let embed_ptx = compile_ptx(LSB_EMBED_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            device
                .load_ptx(embed_ptx, "lsb_embed_mod", &["lsb_embed"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Compile and load LSB extract kernel
            let extract_ptx = compile_ptx(LSB_EXTRACT_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            device
                .load_ptx(extract_ptx, "lsb_extract_mod", &["lsb_extract"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            Ok(Self { device })
        }

        /// Initialise the accelerator on device 0 and compile the full set of
        /// kernels including ML operations (dense layer, SGD, backprop, histogram).
        pub fn new_with_ml() -> Result<Self> {
            let acc = Self::new()?;

            // Dense sigmoid forward pass
            let ptx = compile_ptx(DENSE_SIGMOID_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "dense_sigmoid_mod", &["dense_sigmoid"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // SGD weight update
            let ptx = compile_ptx(SGD_UPDATE_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "sgd_update_mod", &["sgd_update"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Output delta (backprop)
            let ptx = compile_ptx(OUTPUT_DELTA_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "output_delta_mod", &["output_delta"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Weight gradient (outer product)
            let ptx = compile_ptx(WEIGHT_GRAD_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "weight_grad_mod", &["weight_grad"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Hidden layer delta
            let ptx = compile_ptx(HIDDEN_DELTA_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "hidden_delta_mod", &["hidden_delta"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Byte histogram
            let ptx = compile_ptx(BYTE_HISTOGRAM_SRC)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            acc.device
                .load_ptx(ptx, "byte_histogram_mod", &["byte_histogram"])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            Ok(acc)
        }

        // -----------------------------------------------------------------------
        // ML operations
        // -----------------------------------------------------------------------

        /// Forward pass through one dense layer with sigmoid activation.
        ///
        /// `weights`: flattened row-major `[out_size × in_size]`
        /// Returns `out_size` activated values.
        pub fn dense_layer_sigmoid(
            &self,
            weights: &[f32],
            input: &[f32],
            bias: &[f32],
        ) -> Result<Vec<f32>> {
            let out_size = bias.len() as u32;
            let in_size = input.len() as u32;

            let d_w = self.device.htod_sync_copy(weights)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_x = self.device.htod_sync_copy(input)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_b = self.device.htod_sync_copy(bias)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_out = self.device.alloc_zeros::<f32>(out_size as usize)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("dense_sigmoid_mod", "dense_sigmoid")
                .ok_or_else(|| SteganoError::GpuError("dense_sigmoid kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(out_size);
            unsafe { f.launch(cfg, (&d_w, &d_x, &d_b, &mut d_out, in_size, out_size)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device.dtoh_sync_copy(&d_out)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Apply an SGD update: `params[i] -= lr * grads[i]`.  Returns the
        /// updated parameter vector.
        pub fn sgd_update(&self, params: &[f32], grads: &[f32], lr: f32) -> Result<Vec<f32>> {
            let n = params.len() as u32;
            let mut d_p = self.device.htod_sync_copy(params)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_g = self.device.htod_sync_copy(grads)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("sgd_update_mod", "sgd_update")
                .ok_or_else(|| SteganoError::GpuError("sgd_update kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(n);
            unsafe { f.launch(cfg, (&mut d_p, &d_g, lr, n)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device.dtoh_sync_copy(&d_p)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Compute the output-layer backprop delta:
        /// `delta[i] = (output[i] - target[i]) * output[i] * (1 - output[i])`
        pub fn compute_output_delta(&self, output: &[f32], target: &[f32]) -> Result<Vec<f32>> {
            let n = output.len().min(target.len()) as u32;
            let d_out = self.device.htod_sync_copy(&output[..n as usize])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_tgt = self.device.htod_sync_copy(&target[..n as usize])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_delta = self.device.alloc_zeros::<f32>(n as usize)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("output_delta_mod", "output_delta")
                .ok_or_else(|| SteganoError::GpuError("output_delta kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(n);
            unsafe { f.launch(cfg, (&d_out, &d_tgt, &mut d_delta, n)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device.dtoh_sync_copy(&d_delta)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Compute the weight gradient (outer product): `grad[i*in+j] = delta[i]*input[j]`.
        pub fn compute_weight_grad(&self, delta: &[f32], input: &[f32]) -> Result<Vec<f32>> {
            let out_size = delta.len() as u32;
            let in_size = input.len() as u32;
            let total = out_size * in_size;

            let d_delta = self.device.htod_sync_copy(delta)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_input = self.device.htod_sync_copy(input)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_grad = self.device.alloc_zeros::<f32>(total as usize)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("weight_grad_mod", "weight_grad")
                .ok_or_else(|| SteganoError::GpuError("weight_grad kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(total);
            unsafe { f.launch(cfg, (&d_delta, &d_input, &mut d_grad, in_size, out_size)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device.dtoh_sync_copy(&d_grad)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Backpropagate delta through a hidden sigmoid layer.
        ///
        /// `w2` is the weight matrix of the layer *after* the hidden layer
        /// (`[out_size × hidden_size]`). Returns `hidden_size` delta values.
        pub fn compute_hidden_delta(
            &self,
            w2: &[f32],
            out_delta: &[f32],
            hidden: &[f32],
        ) -> Result<Vec<f32>> {
            let hidden_size = hidden.len() as u32;
            let out_size = out_delta.len() as u32;

            let d_w2 = self.device.htod_sync_copy(w2)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_od = self.device.htod_sync_copy(out_delta)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_h = self.device.htod_sync_copy(hidden)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_hd = self.device.alloc_zeros::<f32>(hidden_size as usize)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("hidden_delta_mod", "hidden_delta")
                .ok_or_else(|| SteganoError::GpuError("hidden_delta kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(hidden_size);
            unsafe { f.launch(cfg, (&d_w2, &d_od, &d_h, &mut d_hd, hidden_size, out_size)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device.dtoh_sync_copy(&d_hd)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Build a 256-bucket byte histogram on the GPU and compute Shannon entropy.
        ///
        /// Returns the entropy in bits (0 – 8 for byte data).
        pub fn compute_entropy(&self, data: &[u8]) -> Result<f32> {
            if data.is_empty() {
                return Ok(0.0);
            }
            let n = data.len() as u32;

            let d_data = self.device.htod_sync_copy(data)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_hist = self.device.alloc_zeros::<u32>(256)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self.device.get_func("byte_histogram_mod", "byte_histogram")
                .ok_or_else(|| SteganoError::GpuError("byte_histogram kernel not loaded".into()))?;
            let cfg = LaunchConfig::for_num_elems(n);
            unsafe { f.launch(cfg, (&d_data, &mut d_hist, n)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let hist = self.device.dtoh_sync_copy(&d_hist)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            // Shannon entropy: -sum(p * log2(p))
            let len = data.len() as f32;
            let entropy = hist.iter()
                .filter(|&&c| c > 0)
                .map(|&c| {
                    let p = c as f32 / len;
                    -p * p.log2()
                })
                .sum();

            Ok(entropy)
        }

        /// GPU-accelerated LSB embedding.
        ///
        /// Writes `payload[i] & 1` into the LSB of `data[i]` for `i < n`
        /// where `n = min(payload.len(), data.len())`.
        ///
        /// Returns the number of bytes written.
        pub fn parallel_lsb_embed(&self, data: &mut [u8], payload: &[u8]) -> Result<usize> {
            let n = payload.len().min(data.len()) as u32;
            if n == 0 {
                return Ok(0);
            }

            let mut d_data = self
                .device
                .htod_sync_copy(&data[..n as usize])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let d_payload = self
                .device
                .htod_sync_copy(&payload[..n as usize])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self
                .device
                .get_func("lsb_embed_mod", "lsb_embed")
                .ok_or_else(|| SteganoError::GpuError("lsb_embed kernel not loaded".into()))?;

            let cfg = LaunchConfig::for_num_elems(n);
            unsafe { f.launch(cfg, (&mut d_data, &d_payload, n)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let result = self
                .device
                .dtoh_sync_copy(&d_data)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            data[..n as usize].copy_from_slice(&result);

            Ok(n as usize)
        }

        /// GPU-accelerated LSB extraction.
        ///
        /// Reads the LSB of each byte in `data[..payload_size]` and returns
        /// a `Vec<u8>` where each element is 0 or 1.
        pub fn parallel_lsb_extract(&self, data: &[u8], payload_size: usize) -> Result<Vec<u8>> {
            let n = payload_size.min(data.len()) as u32;
            if n == 0 {
                return Ok(Vec::new());
            }

            let d_data = self
                .device
                .htod_sync_copy(&data[..n as usize])
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;
            let mut d_output = self
                .device
                .alloc_zeros::<u8>(n as usize)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            let f = self
                .device
                .get_func("lsb_extract_mod", "lsb_extract")
                .ok_or_else(|| SteganoError::GpuError("lsb_extract kernel not loaded".into()))?;

            let cfg = LaunchConfig::for_num_elems(n);
            unsafe { f.launch(cfg, (&d_data, &mut d_output, n)) }
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))?;

            self.device
                .dtoh_sync_copy(&d_output)
                .map_err(|e| SteganoError::GpuError(format!("{e:?}")))
        }

        /// Returns the name of the CUDA device in use.
        pub fn device_name(&self) -> String {
            self.device
                .name()
                .unwrap_or_else(|_| "CUDA Device".to_string())
        }
    }

    // -----------------------------------------------------------------------
    // Tests (only compiled when CUDA is present)
    // -----------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_cuda_availability() {
            // Just checks that the probe doesn't panic.
            let _ = CudaAccelerator::is_available();
        }

        #[test]
        #[cfg_attr(not(feature = "cuda"), ignore)]
        fn test_cuda_lsb_roundtrip() {
            if !CudaAccelerator::is_available() {
                eprintln!("No CUDA device found; skipping test");
                return;
            }

            let acc = CudaAccelerator::new().expect("CUDA init");
            println!("CUDA device: {}", acc.device_name());

            let original = vec![0xAAu8; 1024]; // 10101010 pattern
            let payload = vec![0x01u8; 512]; // all-ones payload

            let mut data = original.clone();
            let written = acc
                .parallel_lsb_embed(&mut data, &payload)
                .expect("embed");
            assert_eq!(written, 512);

            // Every embedded byte should have LSB=1
            for i in 0..512 {
                assert_eq!(data[i] & 1, 1, "LSB should be 1 at index {i}");
            }

            // Bytes beyond the payload should be unchanged
            for i in 512..1024 {
                assert_eq!(data[i], 0xAA, "byte {i} should be unmodified");
            }

            // Extract and verify
            let bits = acc.parallel_lsb_extract(&data, 512).expect("extract");
            assert_eq!(bits.len(), 512);
            assert!(bits.iter().all(|&b| b == 1), "all extracted bits should be 1");
        }

        #[test]
        #[cfg_attr(not(feature = "cuda"), ignore)]
        fn test_cuda_large_buffer() {
            if !CudaAccelerator::is_available() {
                return;
            }

            let acc = CudaAccelerator::new().expect("CUDA init");
            let n = 1 << 22; // 4 MB
            let mut data = vec![0u8; n];
            let payload: Vec<u8> = (0..n).map(|i| (i as u8) & 1).collect();

            let written = acc.parallel_lsb_embed(&mut data, &payload).expect("embed");
            assert_eq!(written, n);

            let extracted = acc.parallel_lsb_extract(&data, n).expect("extract");
            assert_eq!(extracted, payload);
        }
    }
} // mod inner

// ---------------------------------------------------------------------------
// Public re-exports — the same symbol is always visible regardless of feature
// ---------------------------------------------------------------------------

#[cfg(feature = "cuda")]
pub use inner::CudaAccelerator;

/// Stub used on non-CUDA targets so the rest of the code compiles uniformly.
#[cfg(not(feature = "cuda"))]
pub struct CudaAccelerator;

#[cfg(not(feature = "cuda"))]
impl CudaAccelerator {
    /// Always returns `false` on non-CUDA targets.
    pub fn is_available() -> bool {
        false
    }
}
