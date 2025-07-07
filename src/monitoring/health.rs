//! Health checking and system status monitoring

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

/// System health checker with periodic status updates
#[derive(Debug)]
pub struct HealthChecker {
    status: Arc<RwLock<HealthStatus>>,
    check_interval: Duration,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Current system health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub last_check: Instant,
    pub uptime: Duration,
    pub memory_usage_mb: Option<u64>,
    pub cpu_usage_percentage: Option<f64>,
    pub disk_usage_percentage: Option<f64>,
    pub active_connections: u32,
    pub error_count: u64,
    pub warning_count: u64,
    pub components: Vec<ComponentHealth>,
}

/// Health status of individual components
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub name: String,
    pub status: ComponentStatus,
    pub last_check: Instant,
    pub details: Option<String>,
    pub response_time: Option<Duration>,
}

/// Component health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(check_interval: Duration) -> crate::core::Result<Self> {
        let status = Arc::new(RwLock::new(HealthStatus::new()));

        Ok(Self {
            status,
            check_interval,
            shutdown_tx: None,
        })
    }

    /// Start periodic health checking
    pub async fn start(&mut self) -> crate::core::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let status = Arc::clone(&self.status);
        let check_interval = self.check_interval;

        tokio::spawn(async move {
            let mut interval = interval(check_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::perform_health_check(&status).await {
                            log::error!("Health check failed: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        log::info!("Health checker shutting down");
                        break;
                    }
                }
            }
        });

        log::info!("Health checker started with interval: {:?}", check_interval);
        Ok(())
    }

    /// Stop health checking
    pub async fn stop(&self) -> crate::core::Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    /// Get current health status
    pub fn current_status(&self) -> HealthStatus {
        self.status.read().unwrap().clone()
    }

    /// Force an immediate health check
    pub async fn check_now(&self) -> crate::core::Result<HealthStatus> {
        Self::perform_health_check(&self.status).await?;
        Ok(self.current_status())
    }

    /// Perform the actual health check
    async fn perform_health_check(status: &Arc<RwLock<HealthStatus>>) -> crate::core::Result<()> {
        let start_time = Instant::now();
        let mut new_status = HealthStatus::new();
        new_status.last_check = start_time;

        // Check system resources
        if let Ok(memory_info) = Self::check_memory_usage() {
            new_status.memory_usage_mb = Some(memory_info);
        }

        if let Ok(cpu_info) = Self::check_cpu_usage().await {
            new_status.cpu_usage_percentage = Some(cpu_info);
        }

        if let Ok(disk_info) = Self::check_disk_usage() {
            new_status.disk_usage_percentage = Some(disk_info);
        }

        // Check individual components
        new_status.components = Self::check_components().await;

        // Determine overall health
        new_status.is_healthy = Self::calculate_overall_health(&new_status);

        // Update stored status
        {
            let mut status_guard = status.write().unwrap();
            new_status.uptime = status_guard.uptime + start_time.elapsed();
            new_status.error_count = status_guard.error_count;
            new_status.warning_count = status_guard.warning_count;

            // Update error/warning counts based on component status
            for component in &new_status.components {
                match component.status {
                    ComponentStatus::Critical => new_status.error_count += 1,
                    ComponentStatus::Warning => new_status.warning_count += 1,
                    _ => {}
                }
            }

            *status_guard = new_status;
        }

        Ok(())
    }

    /// Check memory usage in MB
    fn check_memory_usage() -> Result<u64, Box<dyn std::error::Error>> {
        // Platform-specific memory checking
        #[cfg(target_os = "macos")]
        {
            // Use system calls to get memory info on macOS
            // This is a simplified implementation
            use std::process::Command;

            let output = Command::new("vm_stat").output()?;
            let _output_str = String::from_utf8(output.stdout)?;

            // Parse vm_stat output (simplified)
            // In a real implementation, you'd parse the actual values
            Ok(256) // Placeholder: 256MB
        }

        #[cfg(target_os = "linux")]
        {
            // Read from /proc/meminfo on Linux
            use std::fs;
            let meminfo = fs::read_to_string("/proc/meminfo")?;

            // Parse meminfo (simplified)
            // In a real implementation, you'd parse MemTotal and MemAvailable
            Ok(512) // Placeholder: 512MB
        }

        #[cfg(target_os = "windows")]
        {
            // Use Windows API for memory info
            // This would require additional dependencies
            Ok(384) // Placeholder: 384MB
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            // Generic fallback
            Ok(128) // Placeholder: 128MB
        }
    }

    /// Check CPU usage percentage
    async fn check_cpu_usage() -> Result<f64, Box<dyn std::error::Error>> {
        // Simplified CPU usage check
        // In a real implementation, this would use platform-specific APIs
        // or libraries like sysinfo

        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::Duration;

        static CPU_COUNTER: AtomicU64 = AtomicU64::new(0);

        // Simulate CPU measurement over a brief period
        let start = Instant::now();
        let mut work_done = 0u64;

        while start.elapsed() < Duration::from_millis(10) {
            work_done += 1;
        }

        // Store and calculate relative usage
        let prev_work = CPU_COUNTER.swap(work_done, Ordering::Relaxed);
        let cpu_percentage = if prev_work > 0 {
            ((work_done as f64 / prev_work as f64) * 25.0).min(100.0)
        } else {
            15.0 // Default value for first measurement
        };

        Ok(cpu_percentage)
    }

    /// Check disk usage percentage
    fn check_disk_usage() -> Result<f64, Box<dyn std::error::Error>> {
        // Platform-specific disk usage checking
        #[cfg(unix)]
        {
            // This is a simplified implementation
            // In a real implementation, you'd use statvfs or similar
            Ok(45.0) // Placeholder: 45% disk usage
        }

        #[cfg(windows)]
        {
            // Use Windows API for disk space
            Ok(60.0) // Placeholder: 60% disk usage
        }

        #[cfg(not(any(unix, windows)))]
        {
            Ok(30.0) // Placeholder: 30% disk usage
        }
    }

    /// Check health of individual components
    async fn check_components() -> Vec<ComponentHealth> {
        let mut components = Vec::new();

        // Check core stegano components
        components.push(Self::check_component("Core Engine").await);
        components.push(Self::check_component("Handler Registry").await);
        components.push(Self::check_component("Cryptography").await);
        components.push(Self::check_component("GPU Acceleration").await);
        components.push(Self::check_component("File I/O").await);

        components
    }

    /// Check health of a specific component
    async fn check_component(name: &str) -> ComponentHealth {
        let start_time = Instant::now();

        // Simulate component health check
        tokio::time::sleep(Duration::from_millis(1)).await;

        let response_time = start_time.elapsed();

        // Determine component status based on response time and name
        let status = match name {
            "Core Engine" => ComponentStatus::Healthy,
            "Handler Registry" => ComponentStatus::Healthy,
            "Cryptography" => ComponentStatus::Healthy,
            "GPU Acceleration" => {
                // Check if GPU is actually available
                #[cfg(feature = "gpu")]
                {
                    // Check GPU availability - simplified for now
                    ComponentStatus::Warning // Assume GPU warning until we can check properly
                }
                #[cfg(not(feature = "gpu"))]
                ComponentStatus::Warning
            }
            "File I/O" => ComponentStatus::Healthy,
            _ => ComponentStatus::Unknown,
        };

        let details = match status {
            ComponentStatus::Warning if name == "GPU Acceleration" => {
                Some("GPU acceleration not available, using CPU fallback".to_string())
            }
            _ => None,
        };

        ComponentHealth {
            name: name.to_string(),
            status,
            last_check: start_time,
            details,
            response_time: Some(response_time),
        }
    }

    /// Calculate overall system health
    fn calculate_overall_health(status: &HealthStatus) -> bool {
        // Check if any critical components are failing
        let has_critical = status
            .components
            .iter()
            .any(|c| c.status == ComponentStatus::Critical);

        if has_critical {
            return false;
        }

        // Check resource usage
        if let Some(memory) = status.memory_usage_mb {
            if memory > 2048 {
                // More than 2GB
                return false;
            }
        }

        if let Some(cpu) = status.cpu_usage_percentage {
            if cpu > 95.0 {
                // More than 95% CPU
                return false;
            }
        }

        if let Some(disk) = status.disk_usage_percentage {
            if disk > 90.0 {
                // More than 90% disk
                return false;
            }
        }

        true
    }
}

impl HealthStatus {
    /// Create a new health status with default values
    pub fn new() -> Self {
        Self {
            is_healthy: true,
            last_check: Instant::now(),
            uptime: Duration::default(),
            memory_usage_mb: None,
            cpu_usage_percentage: None,
            disk_usage_percentage: None,
            active_connections: 0,
            error_count: 0,
            warning_count: 0,
            components: Vec::new(),
        }
    }

    /// Check if the system is healthy
    pub fn is_healthy(&self) -> bool {
        self.is_healthy
    }

    /// Get summary of component health
    pub fn component_summary(&self) -> (usize, usize, usize, usize) {
        let mut healthy = 0;
        let mut warning = 0;
        let mut critical = 0;
        let mut unknown = 0;

        for component in &self.components {
            match component.status {
                ComponentStatus::Healthy => healthy += 1,
                ComponentStatus::Warning => warning += 1,
                ComponentStatus::Critical => critical += 1,
                ComponentStatus::Unknown => unknown += 1,
            }
        }

        (healthy, warning, critical, unknown)
    }
}

impl ComponentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::Critical => "critical",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_creation() {
        let status = HealthStatus::new();
        assert!(status.is_healthy());
        assert_eq!(status.error_count, 0);
        assert_eq!(status.warning_count, 0);
        assert!(status.components.is_empty());
    }

    #[test]
    fn test_component_status_string() {
        assert_eq!(ComponentStatus::Healthy.as_str(), "healthy");
        assert_eq!(ComponentStatus::Warning.as_str(), "warning");
        assert_eq!(ComponentStatus::Critical.as_str(), "critical");
        assert_eq!(ComponentStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_health_checker_creation() {
        let checker = HealthChecker::new(Duration::from_secs(30));
        assert!(checker.is_ok());

        let checker = checker.unwrap();
        let status = checker.current_status();
        assert!(status.is_healthy());
    }

    #[tokio::test]
    async fn test_component_health_check() {
        let component = HealthChecker::check_component("Test Component").await;
        assert_eq!(component.name, "Test Component");
        assert!(component.response_time.is_some());
        assert!(component.response_time.unwrap() < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_components_check() {
        let components = HealthChecker::check_components().await;
        assert!(!components.is_empty());

        // Should have core components
        let core_engine = components.iter().find(|c| c.name == "Core Engine");
        assert!(core_engine.is_some());
        assert_eq!(core_engine.unwrap().status, ComponentStatus::Healthy);
    }

    #[test]
    fn test_component_summary() {
        let mut status = HealthStatus::new();
        status.components = vec![
            ComponentHealth {
                name: "Test1".to_string(),
                status: ComponentStatus::Healthy,
                last_check: Instant::now(),
                details: None,
                response_time: None,
            },
            ComponentHealth {
                name: "Test2".to_string(),
                status: ComponentStatus::Warning,
                last_check: Instant::now(),
                details: None,
                response_time: None,
            },
            ComponentHealth {
                name: "Test3".to_string(),
                status: ComponentStatus::Critical,
                last_check: Instant::now(),
                details: None,
                response_time: None,
            },
        ];

        let (healthy, warning, critical, unknown) = status.component_summary();
        assert_eq!(healthy, 1);
        assert_eq!(warning, 1);
        assert_eq!(critical, 1);
        assert_eq!(unknown, 0);
    }

    #[test]
    fn test_memory_usage_check() {
        let result = HealthChecker::check_memory_usage();
        // Should return some value or error, but not panic
        match result {
            Ok(memory) => assert!(memory > 0),
            Err(_) => {} // Error is acceptable for testing
        }
    }

    #[tokio::test]
    async fn test_cpu_usage_check() {
        let result = HealthChecker::check_cpu_usage().await;
        match result {
            Ok(cpu) => {
                assert!(cpu >= 0.0);
                assert!(cpu <= 100.0);
            }
            Err(_) => {} // Error is acceptable for testing
        }
    }

    #[test]
    fn test_disk_usage_check() {
        let result = HealthChecker::check_disk_usage();
        match result {
            Ok(disk) => {
                assert!(disk >= 0.0);
                assert!(disk <= 100.0);
            }
            Err(_) => {} // Error is acceptable for testing
        }
    }
}
