//! GPU kernel definitions

use std::collections::HashMap;

pub struct KernelManager {
    kernels: HashMap<String, KernelDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KernelDefinition {
    pub name: String,
    pub source: String,
    pub entry_point: String,
    pub workgroup_size: (u32, u32, u32),
}

impl KernelManager {
    pub fn new() -> Self {
        let mut manager = Self {
            kernels: HashMap::new(),
        };
        
        // Initialize with built-in kernels
        manager.load_builtin_kernels();
        manager
    }
    
    /// Load built-in GPU kernels
    fn load_builtin_kernels(&mut self) {
        // LSB embedding kernel
        self.register_kernel(KernelDefinition {
            name: "lsb_embed".to_string(),
            source: include_str!("shaders/steganography.metal").to_string(),
            entry_point: "lsb_embed_kernel".to_string(),
            workgroup_size: (256, 1, 1),
        });
        
        // FFT kernel
        self.register_kernel(KernelDefinition {
            name: "fft".to_string(),
            source: include_str!("shaders/steganography.metal").to_string(),
            entry_point: "fft_kernel".to_string(),
            workgroup_size: (256, 1, 1),
        });
        
        // Reed-Solomon kernel
        self.register_kernel(KernelDefinition {
            name: "reed_solomon".to_string(),
            source: include_str!("shaders/steganography.metal").to_string(),
            entry_point: "reed_solomon_kernel".to_string(),
            workgroup_size: (64, 1, 1),
        });
    }
    
    /// Register a new kernel
    pub fn register_kernel(&mut self, kernel: KernelDefinition) {
        self.kernels.insert(kernel.name.clone(), kernel);
    }
    
    /// Get a kernel by name
    pub fn get_kernel(&self, name: &str) -> Option<&KernelDefinition> {
        self.kernels.get(name)
    }
    
    /// List all available kernels
    pub fn list_kernels(&self) -> Vec<&str> {
        self.kernels.keys().map(|s| s.as_str()).collect()
    }
    
    /// Check if a kernel exists
    pub fn has_kernel(&self, name: &str) -> bool {
        self.kernels.contains_key(name)
    }
    
    /// Remove a kernel
    pub fn remove_kernel(&mut self, name: &str) -> Option<KernelDefinition> {
        self.kernels.remove(name)
    }
    
    /// Get kernel count
    pub fn kernel_count(&self) -> usize {
        self.kernels.len()
    }
}

impl Default for KernelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelDefinition {
    pub fn new(name: String, source: String, entry_point: String) -> Self {
        Self {
            name,
            source,
            entry_point,
            workgroup_size: (256, 1, 1), // Default workgroup size
        }
    }
    
    pub fn with_workgroup_size(mut self, size: (u32, u32, u32)) -> Self {
        self.workgroup_size = size;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_kernel_manager_creation() {
        let manager = KernelManager::new();
        assert!(manager.kernel_count() > 0);
    }
    
    #[test]
    fn test_kernel_manager_default() {
        let manager = KernelManager::default();
        assert!(manager.has_kernel("lsb_embed"));
    }
    
    #[test]
    fn test_builtin_kernels() {
        let manager = KernelManager::new();
        
        // Should have built-in kernels
        assert!(manager.has_kernel("lsb_embed"));
        assert!(manager.has_kernel("fft"));
        assert!(manager.has_kernel("reed_solomon"));
        
        // Should be at least 3 kernels
        assert!(manager.kernel_count() >= 3);
    }
    
    #[test]
    fn test_kernel_registration() {
        let mut manager = KernelManager::new();
        let initial_count = manager.kernel_count();
        
        let custom_kernel = KernelDefinition::new(
            "custom_test".to_string(),
            "kernel void custom_test() {}".to_string(),
            "custom_test".to_string(),
        );
        
        manager.register_kernel(custom_kernel);
        
        assert!(manager.has_kernel("custom_test"));
        assert_eq!(manager.kernel_count(), initial_count + 1);
    }
    
    #[test]
    fn test_kernel_retrieval() {
        let manager = KernelManager::new();
        
        let lsb_kernel = manager.get_kernel("lsb_embed");
        assert!(lsb_kernel.is_some());
        
        let kernel = lsb_kernel.unwrap();
        assert_eq!(kernel.name, "lsb_embed");
        assert_eq!(kernel.entry_point, "lsb_embed_kernel");
        assert_eq!(kernel.workgroup_size, (256, 1, 1));
        
        // Non-existent kernel should return None
        let missing = manager.get_kernel("non_existent");
        assert!(missing.is_none());
    }
    
    #[test]
    fn test_kernel_listing() {
        let manager = KernelManager::new();
        let kernels = manager.list_kernels();
        
        assert!(kernels.contains(&"lsb_embed"));
        assert!(kernels.contains(&"fft"));
        assert!(kernels.contains(&"reed_solomon"));
        assert!(kernels.len() >= 3);
    }
    
    #[test]
    fn test_kernel_removal() {
        let mut manager = KernelManager::new();
        
        // Add a custom kernel
        let custom_kernel = KernelDefinition::new(
            "removable".to_string(),
            "test source".to_string(),
            "test_entry".to_string(),
        );
        manager.register_kernel(custom_kernel);
        
        assert!(manager.has_kernel("removable"));
        
        // Remove it
        let removed = manager.remove_kernel("removable");
        assert!(removed.is_some());
        assert!(!manager.has_kernel("removable"));
        
        // Removing non-existent kernel should return None
        let missing = manager.remove_kernel("non_existent");
        assert!(missing.is_none());
    }
    
    #[test]
    fn test_kernel_definition() {
        let kernel = KernelDefinition::new(
            "test_kernel".to_string(),
            "kernel void test() {}".to_string(),
            "test".to_string(),
        );
        
        assert_eq!(kernel.name, "test_kernel");
        assert_eq!(kernel.source, "kernel void test() {}");
        assert_eq!(kernel.entry_point, "test");
        assert_eq!(kernel.workgroup_size, (256, 1, 1));
    }
    
    #[test]
    fn test_kernel_definition_with_workgroup_size() {
        let kernel = KernelDefinition::new(
            "sized_kernel".to_string(),
            "kernel void sized() {}".to_string(),
            "sized".to_string(),
        ).with_workgroup_size((512, 2, 1));
        
        assert_eq!(kernel.workgroup_size, (512, 2, 1));
    }
    
    #[test]
    fn test_kernel_definition_clone() {
        let original = KernelDefinition::new(
            "clone_test".to_string(),
            "source".to_string(),
            "entry".to_string(),
        );
        
        let cloned = original.clone();
        assert_eq!(original, cloned);
        assert_eq!(original.name, cloned.name);
    }
    
    #[test]
    fn test_kernel_manager_override() {
        let mut manager = KernelManager::new();
        
        // Override an existing kernel
        let custom_lsb = KernelDefinition::new(
            "lsb_embed".to_string(),
            "custom LSB implementation".to_string(),
            "custom_lsb_kernel".to_string(),
        );
        
        let original_count = manager.kernel_count();
        manager.register_kernel(custom_lsb);
        
        // Count should remain the same (override, not add)
        assert_eq!(manager.kernel_count(), original_count);
        
        // But the kernel should be updated
        let updated_kernel = manager.get_kernel("lsb_embed").unwrap();
        assert_eq!(updated_kernel.source, "custom LSB implementation");
        assert_eq!(updated_kernel.entry_point, "custom_lsb_kernel");
    }
}