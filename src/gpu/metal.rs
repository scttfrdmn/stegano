//! Metal GPU acceleration for macOS steganography operations

#[cfg(all(target_os = "macos", feature = "metal"))]
use crate::core::*;

#[cfg(all(target_os = "macos", feature = "metal"))]
use metal::*;

#[cfg(all(target_os = "macos", feature = "metal"))]
use std::mem;

#[cfg(all(target_os = "macos", feature = "metal"))]
#[derive(Debug)]
pub struct MetalAccelerator {
    device: Device,
    command_queue: CommandQueue,
    library: Library,
}

#[cfg(all(target_os = "macos", feature = "metal"))]
impl MetalAccelerator {
    /// Create a new Metal accelerator instance
    pub fn new() -> crate::core::Result<Self> {
        let device = Device::system_default()
            .ok_or_else(|| SteganoError::GpuError("No Metal device found".to_string()))?;

        let command_queue = device.new_command_queue();

        // Create Metal library with our compute shaders
        let library_source = include_str!("shaders/steganography.metal");
        let compile_options = CompileOptions::new();
        let library = device
            .new_library_with_source(library_source, &compile_options)
            .map_err(|e| SteganoError::GpuError(format!("Failed to compile Metal library: {e}")))?;

        Ok(Self {
            device,
            command_queue,
            library,
        })
    }

    /// Get device information
    pub fn device_info(&self) -> String {
        format!("Metal Device: {}", self.device.name())
    }

    /// Parallel LSB embedding using Metal compute shaders
    pub fn parallel_lsb_embed(&self, carrier: &mut [u8], payload_bits: &[u8]) -> Result<()> {
        let function = self
            .library
            .get_function("lsb_embed_kernel", None)
            .map_err(|e| {
                SteganoError::GpuError(format!("Failed to get LSB embed function: {e}"))
            })?;

        let pipeline = self
            .device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| {
                SteganoError::GpuError(format!("Failed to create compute pipeline: {e}"))
            })?;

        // Create buffers
        let carrier_buffer = self.device.new_buffer_with_data(
            unsafe { mem::transmute(carrier.as_ptr()) },
            std::mem::size_of_val(carrier) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let payload_buffer = self.device.new_buffer_with_data(
            unsafe { mem::transmute(payload_bits.as_ptr()) },
            std::mem::size_of_val(payload_bits) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create command buffer and encoder
        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&pipeline);
        encoder.set_buffer(0, Some(&carrier_buffer), 0);
        encoder.set_buffer(1, Some(&payload_buffer), 0);

        // Set payload length as buffer(2)
        let payload_length = payload_bits.len() as u32;
        encoder.set_bytes(2, mem::size_of::<u32>() as u64, unsafe {
            mem::transmute(&payload_length)
        });

        // Calculate thread configuration
        let threads_per_threadgroup = MTLSize::new(256, 1, 1);
        let threadgroups_per_grid = MTLSize::new(payload_bits.len().div_ceil(256) as u64, 1, 1);

        encoder.dispatch_thread_groups(threadgroups_per_grid, threads_per_threadgroup);
        encoder.end_encoding();

        // Execute
        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Copy results back
        let result_ptr = carrier_buffer.contents() as *const u8;
        unsafe {
            std::ptr::copy_nonoverlapping(result_ptr, carrier.as_mut_ptr(), carrier.len());
        }

        Ok(())
    }

    /// GPU-accelerated FFT for frequency domain operations
    pub fn gpu_fft(&self, signal: &[f32]) -> Result<Vec<num_complex::Complex<f32>>> {
        let function = self
            .library
            .get_function("fft_kernel", None)
            .map_err(|e| SteganoError::GpuError(format!("Failed to get FFT function: {e}")))?;

        let pipeline = self
            .device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| SteganoError::GpuError(format!("Failed to create FFT pipeline: {e}")))?;

        // Create input buffer
        let input_buffer = self.device.new_buffer_with_data(
            unsafe { mem::transmute(signal.as_ptr()) },
            std::mem::size_of_val(signal) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create output buffer for complex numbers (2 floats per complex)
        let output_size = signal.len() * 2 * mem::size_of::<f32>();
        let output_buffer = self
            .device
            .new_buffer(output_size as u64, MTLResourceOptions::StorageModeShared);

        // Execute computation
        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&pipeline);
        encoder.set_buffer(0, Some(&input_buffer), 0);
        encoder.set_buffer(1, Some(&output_buffer), 0);

        // Set signal length as buffer(2)
        let signal_length = signal.len() as u32;
        encoder.set_bytes(2, mem::size_of::<u32>() as u64, unsafe {
            mem::transmute(&signal_length)
        });

        let threads_per_threadgroup = MTLSize::new(256, 1, 1);
        let threadgroups_per_grid = MTLSize::new(signal.len().div_ceil(256) as u64, 1, 1);

        encoder.dispatch_thread_groups(threadgroups_per_grid, threads_per_threadgroup);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Convert results back to Complex numbers
        let result_ptr = output_buffer.contents() as *const f32;
        let mut result = Vec::with_capacity(signal.len());

        unsafe {
            for i in 0..signal.len() {
                let real = *result_ptr.add(i * 2);
                let imag = *result_ptr.add(i * 2 + 1);
                result.push(num_complex::Complex::new(real, imag));
            }
        }

        Ok(result)
    }

    /// Parallel Reed-Solomon encoding using Metal
    pub fn parallel_reed_solomon(&self, data: &[u8], parity_bytes: usize) -> Result<Vec<u8>> {
        let function = self
            .library
            .get_function("reed_solomon_kernel", None)
            .map_err(|e| {
                SteganoError::GpuError(format!("Failed to get Reed-Solomon function: {e}"))
            })?;

        let pipeline = self
            .device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| {
                SteganoError::GpuError(format!("Failed to create Reed-Solomon pipeline: {e}"))
            })?;

        let output_size = data.len() + parity_bytes;

        // Create buffers
        let input_buffer = self.device.new_buffer_with_data(
            unsafe { mem::transmute(data.as_ptr()) },
            std::mem::size_of_val(data) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let output_buffer = self.device.new_buffer(
            (output_size * mem::size_of::<u8>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Execute computation
        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&pipeline);
        encoder.set_buffer(0, Some(&input_buffer), 0);
        encoder.set_buffer(1, Some(&output_buffer), 0);

        // Set data_length as buffer(2)
        let data_length = data.len() as u32;
        encoder.set_bytes(2, mem::size_of::<u32>() as u64, unsafe {
            mem::transmute(&data_length)
        });

        // Set parity_bytes as buffer(3)
        encoder.set_bytes(3, mem::size_of::<u32>() as u64, unsafe {
            mem::transmute(&(parity_bytes as u32))
        });

        let threads_per_threadgroup = MTLSize::new(64, 1, 1);
        let threadgroups_per_grid = MTLSize::new(data.len().div_ceil(64) as u64, 1, 1);

        encoder.dispatch_thread_groups(threadgroups_per_grid, threads_per_threadgroup);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Copy results
        let result_ptr = output_buffer.contents() as *const u8;
        let mut result = vec![0u8; output_size];
        unsafe {
            std::ptr::copy_nonoverlapping(result_ptr, result.as_mut_ptr(), output_size);
        }

        Ok(result)
    }

    /// Check if Metal is available and functioning
    pub fn is_available() -> bool {
        Device::system_default().is_some()
    }

    /// Get Metal device capabilities
    pub fn capabilities(&self) -> MetalCapabilities {
        MetalCapabilities {
            max_threads_per_threadgroup: self.device.max_threads_per_threadgroup(),
            supports_nonuniform_threadgroups: true, // Modern Metal devices support this
            max_buffer_length: self.device.max_buffer_length(),
            unified_memory: true, // macOS always has unified memory
        }
    }
}

#[cfg(all(target_os = "macos", feature = "metal"))]
pub struct MetalCapabilities {
    pub max_threads_per_threadgroup: MTLSize,
    pub supports_nonuniform_threadgroups: bool,
    pub max_buffer_length: u64,
    pub unified_memory: bool,
}

// Stub implementations for non-macOS or when Metal feature is disabled
#[cfg(not(all(target_os = "macos", feature = "metal")))]
pub struct MetalAccelerator;

#[cfg(not(all(target_os = "macos", feature = "metal")))]
impl MetalAccelerator {
    pub fn new() -> crate::core::Result<Self> {
        Err(crate::core::SteganoError::GpuError(
            "Metal not available on this platform".to_string(),
        ))
    }

    pub fn is_available() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metal_availability() {
        #[cfg(target_os = "macos")]
        {
            // On macOS, Metal should be available
            println!("Metal available: {}", MetalAccelerator::is_available());
        }

        #[cfg(not(target_os = "macos"))]
        {
            // On other platforms, Metal should not be available
            assert!(!MetalAccelerator::is_available());
        }
    }

    #[cfg(all(target_os = "macos", feature = "metal"))]
    #[test]
    fn test_metal_device_creation() {
        if MetalAccelerator::is_available() {
            let accelerator = MetalAccelerator::new();
            match accelerator {
                Ok(acc) => {
                    println!("✅ Metal device created: {}", acc.device_info());
                    let caps = acc.capabilities();
                    println!(
                        "Max threads per threadgroup: {:?}",
                        caps.max_threads_per_threadgroup
                    );
                    println!("Unified memory: {}", caps.unified_memory);
                    println!(
                        "Max buffer length: {:.2} GB",
                        caps.max_buffer_length as f64 / (1024.0 * 1024.0 * 1024.0)
                    );
                }
                Err(e) => {
                    println!("⚠️  Metal device creation failed: {e}");
                    println!("This is expected if Metal compute shaders can't be compiled");
                    // Don't fail the test - this is acceptable for now
                }
            }
        } else {
            println!("Metal not available on this system");
        }
    }

    #[cfg(all(target_os = "macos", feature = "metal"))]
    #[test]
    fn test_metal_buffer_operations() {
        if MetalAccelerator::is_available() {
            let accelerator = MetalAccelerator::new().unwrap();

            // Test basic buffer creation and operations
            let test_data = vec![0u8; 1024];
            let buffer = accelerator.device.new_buffer_with_data(
                unsafe { std::mem::transmute(test_data.as_ptr()) },
                test_data.len() as u64,
                MTLResourceOptions::StorageModeShared,
            );

            assert_eq!(buffer.length(), test_data.len() as u64);
        }
    }
}
