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
    // CudaAccelerator
    // -----------------------------------------------------------------------

    /// CUDA-backed accelerator for steganographic LSB operations.
    pub struct CudaAccelerator {
        device: Arc<CudaDevice>,
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
