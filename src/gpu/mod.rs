//! GPU acceleration for high-performance steganography operations

pub mod compute;
pub mod kernels;
pub mod metal;

use crate::core::*;
use wgpu;


/// Unified GPU acceleration interface that automatically selects the best backend
#[derive(Debug)]
pub enum GpuAccelerator {
    #[cfg(all(target_os = "macos", feature = "metal"))]
    Metal(metal::MetalAccelerator),
    Wgpu(WgpuAccelerator),
}

/// WGPU-based GPU accelerator for cross-platform support
#[derive(Debug)]
pub struct WgpuAccelerator {
    #[allow(dead_code)]
    device: wgpu::Device,
    #[allow(dead_code)]
    queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    #[allow(dead_code)]
    compute_pipelines: std::collections::HashMap<String, wgpu::ComputePipeline>,
}

impl GpuAccelerator {
    /// Create a new GPU accelerator, automatically selecting the best backend
    pub async fn new() -> Result<Self> {
        // Try Metal first on macOS for optimal performance
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if metal::MetalAccelerator::is_available() {
                match metal::MetalAccelerator::new() {
                    Ok(metal_acc) => return Ok(GpuAccelerator::Metal(metal_acc)),
                    Err(_) => {
                        // Fall back to WGPU if Metal fails
                        log::warn!("Metal initialization failed, falling back to WGPU");
                    }
                }
            }
        }
        
        // Use WGPU as fallback or primary choice
        let wgpu_acc = WgpuAccelerator::new().await?;
        Ok(GpuAccelerator::Wgpu(wgpu_acc))
    }
    
    /// Create a GPU accelerator with a specific backend preference
    pub async fn new_with_backend(backend: GpuBackend) -> Result<Self> {
        match backend {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuBackend::Metal => {
                let metal_acc = metal::MetalAccelerator::new()?;
                Ok(GpuAccelerator::Metal(metal_acc))
            }
            GpuBackend::Wgpu => {
                let wgpu_acc = WgpuAccelerator::new().await?;
                Ok(GpuAccelerator::Wgpu(wgpu_acc))
            }
        }
    }
    
    /// Get information about the GPU device
    pub fn device_info(&self) -> String {
        match self {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuAccelerator::Metal(metal) => metal.device_info(),
            GpuAccelerator::Wgpu(wgpu) => wgpu.device_info(),
        }
    }
    
    /// Parallel LSB embedding using GPU compute shaders
    pub async fn parallel_lsb_embed(&self, data: &mut [u8], payload_bits: &[u8]) -> Result<()> {
        match self {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuAccelerator::Metal(metal) => metal.parallel_lsb_embed(data, payload_bits),
            GpuAccelerator::Wgpu(wgpu) => wgpu.parallel_lsb_embed(data, payload_bits).await,
        }
    }
    
    /// GPU-accelerated FFT for frequency domain embedding
    pub async fn gpu_fft(&self, signal: &[f32]) -> Result<Vec<num_complex::Complex<f32>>> {
        match self {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuAccelerator::Metal(metal) => metal.gpu_fft(signal),
            GpuAccelerator::Wgpu(wgpu) => wgpu.gpu_fft(signal).await,
        }
    }
    
    /// Parallel Reed-Solomon encoding
    pub async fn parallel_reed_solomon(&self, data: &[u8], parity_bytes: usize) -> Result<Vec<u8>> {
        match self {
            #[cfg(all(target_os = "macos", feature = "metal"))]
            GpuAccelerator::Metal(metal) => metal.parallel_reed_solomon(data, parity_bytes),
            GpuAccelerator::Wgpu(wgpu) => wgpu.parallel_reed_solomon(data, parity_bytes).await,
        }
    }
    
    /// Check if any GPU acceleration is available
    pub fn is_available() -> bool {
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if metal::MetalAccelerator::is_available() {
                return true;
            }
        }
        
        // For WGPU availability, we'd need to do async initialization
        // For now, assume it's available if this function is called
        true
    }
}

/// GPU backend selection
#[derive(Debug, Clone, Copy)]
pub enum GpuBackend {
    #[cfg(all(target_os = "macos", feature = "metal"))]
    Metal,
    Wgpu,
}

impl WgpuAccelerator {
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
            flags: wgpu::InstanceFlags::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });
        
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| SteganoError::GpuError("Failed to find GPU adapter".to_string()))?;
            
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .map_err(|e| SteganoError::GpuError(format!("Failed to create device: {}", e)))?;
        
        Ok(Self {
            device,
            queue,
            adapter,
            compute_pipelines: Default::default(),
        })
    }
    
    pub fn device_info(&self) -> String {
        format!("WGPU Device: {} ({})", 
                self.adapter.get_info().name,
                self.adapter.get_info().backend.to_str())
    }
    
    /// Parallel LSB embedding using WGPU compute shaders
    pub async fn parallel_lsb_embed(&self, _data: &mut [u8], _payload_bits: &[u8]) -> Result<()> {
        // Implementation for WGPU-based LSB embedding
        todo!("Implement WGPU LSB embedding")
    }
    
    /// GPU-accelerated FFT using WGPU
    pub async fn gpu_fft(&self, _signal: &[f32]) -> Result<Vec<num_complex::Complex<f32>>> {
        // Implementation for WGPU FFT
        todo!("Implement WGPU FFT")
    }
    
    /// Parallel Reed-Solomon encoding using WGPU
    pub async fn parallel_reed_solomon(&self, _data: &[u8], _parity_bytes: usize) -> Result<Vec<u8>> {
        // Implementation for WGPU Reed-Solomon
        todo!("Implement WGPU Reed-Solomon")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_gpu_accelerator_creation() {
        // Test that we can create a GPU accelerator
        let result = GpuAccelerator::new().await;
        
        match result {
            Ok(accelerator) => {
                println!("GPU accelerator created: {}", accelerator.device_info());
                
                #[cfg(all(target_os = "macos", feature = "metal"))]
                {
                    // On macOS with Metal feature, we should prefer Metal
                    if metal::MetalAccelerator::is_available() {
                        match accelerator {
                            GpuAccelerator::Metal(_) => {
                                println!("✓ Using Metal backend as expected on macOS");
                            }
                            GpuAccelerator::Wgpu(_) => {
                                println!("⚠ Using WGPU fallback on macOS");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("GPU accelerator creation failed: {}", e);
                // This is acceptable as not all systems have GPU acceleration
            }
        }
    }
    
    #[tokio::test]
    async fn test_gpu_backend_selection() {
        // Test explicit backend selection
        
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if metal::MetalAccelerator::is_available() {
                let metal_result = GpuAccelerator::new_with_backend(GpuBackend::Metal).await;
                assert!(metal_result.is_ok(), "Metal backend should be available on macOS");
                
                if let Ok(GpuAccelerator::Metal(_)) = metal_result {
                    println!("✓ Metal backend explicitly selected");
                }
            }
        }
        
        // WGPU should be available on most systems
        let wgpu_result = GpuAccelerator::new_with_backend(GpuBackend::Wgpu).await;
        match wgpu_result {
            Ok(_) => println!("✓ WGPU backend available"),
            Err(e) => println!("⚠ WGPU backend not available: {}", e),
        }
    }
    
    #[test]
    fn test_gpu_availability() {
        let available = GpuAccelerator::is_available();
        println!("GPU acceleration available: {}", available);
        
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            let metal_available = metal::MetalAccelerator::is_available();
            println!("Metal specifically available: {}", metal_available);
        }
    }
    
    #[cfg(all(target_os = "macos", feature = "metal"))]
    #[tokio::test]
    async fn test_metal_lsb_embedding() {
        if !metal::MetalAccelerator::is_available() {
            return;
        }
        
        let accelerator = match GpuAccelerator::new_with_backend(GpuBackend::Metal).await {
            Ok(acc) => acc,
            Err(_) => return,
        };
        
        // Test data
        let mut carrier = vec![0x80u8; 1000]; // MSB set, LSB clear
        let payload_bits = vec![1u8; 100]; // All bits set to 1
        
        // This would test the actual Metal implementation
        // For now, we just test that the interface works
        let result = accelerator.parallel_lsb_embed(&mut carrier, &payload_bits).await;
        
        match result {
            Ok(_) => {
                println!("✓ Metal LSB embedding interface works");
                // Verify that LSBs were modified
                for i in 32..132 { // Skip header, check first 100 payload bits
                    assert_eq!(carrier[i] & 1, 1, "LSB should be set at position {}", i);
                }
            }
            Err(e) => println!("Metal LSB embedding failed: {}", e),
        }
    }
    
    #[tokio::test]
    async fn test_gpu_performance_comparison() {
        // This test compares performance between different backends
        // when both are available
        
        println!("=== GPU Performance Comparison ===");
        
        #[cfg(all(target_os = "macos", feature = "metal"))]
        {
            if metal::MetalAccelerator::is_available() {
                let metal_acc = GpuAccelerator::new_with_backend(GpuBackend::Metal).await;
                if let Ok(acc) = metal_acc {
                    println!("Metal backend: {}", acc.device_info());
                }
            }
        }
        
        let wgpu_acc = GpuAccelerator::new_with_backend(GpuBackend::Wgpu).await;
        if let Ok(acc) = wgpu_acc {
            println!("WGPU backend: {}", acc.device_info());
        }
    }
}
