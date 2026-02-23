//! Production monitoring and observability for stegano
//!
//! This module provides comprehensive monitoring, health checks, metrics collection,
//! and observability features for production deployments of stegano.

pub mod health;
pub mod logging;
pub mod metrics;
pub mod telemetry;

use crate::core::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Central monitoring system for production stegano deployments
#[derive(Debug, Clone)]
pub struct MonitoringSystem {
    health_checker: Arc<std::sync::Mutex<health::HealthChecker>>,
    metrics_collector: Arc<std::sync::Mutex<metrics::MetricsCollector>>,
    logger: Arc<logging::StructuredLogger>,
    telemetry: Arc<std::sync::Mutex<telemetry::TelemetryCollector>>,
    config: MonitoringConfig,
}

/// Configuration for the monitoring system
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    /// Metrics collection interval
    pub metrics_interval: Duration,
    /// Enable performance profiling
    pub enable_profiling: bool,
    /// Enable detailed request tracing
    pub enable_tracing: bool,
    /// Maximum number of metrics to retain
    pub metrics_retention_limit: usize,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Alert threshold configuration
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Maximum acceptable error rate (0.0 to 1.0)
    pub max_error_rate: f64,
    /// Maximum acceptable response time in milliseconds
    pub max_response_time_ms: u64,
    /// Maximum memory usage in MB
    pub max_memory_mb: u64,
    /// Maximum CPU usage percentage (0.0 to 100.0)
    pub max_cpu_percentage: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            metrics_interval: Duration::from_secs(60),
            enable_profiling: true,
            enable_tracing: false, // Disabled by default for performance
            metrics_retention_limit: 10000,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_error_rate: 0.05,       // 5% error rate
            max_response_time_ms: 5000, // 5 seconds
            max_memory_mb: 1024,        // 1GB
            max_cpu_percentage: 80.0,   // 80% CPU
        }
    }
}

impl MonitoringSystem {
    /// Initialize the monitoring system with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(MonitoringConfig::default())
    }

    /// Initialize the monitoring system with custom configuration
    pub fn with_config(config: MonitoringConfig) -> Result<Self> {
        let health_checker = Arc::new(std::sync::Mutex::new(health::HealthChecker::new(
            config.health_check_interval,
        )?));
        let metrics_collector = Arc::new(std::sync::Mutex::new(metrics::MetricsCollector::new(
            config.metrics_retention_limit,
        )?));
        let logger = Arc::new(logging::StructuredLogger::new()?);
        let telemetry = Arc::new(std::sync::Mutex::new(telemetry::TelemetryCollector::new()?));

        log::info!("Monitoring system initialized with config: {config:?}");

        Ok(Self {
            health_checker,
            metrics_collector,
            logger,
            telemetry,
            config,
        })
    }

    /// Start the monitoring system background tasks
    #[allow(clippy::await_holding_lock)]
    pub async fn start(&self) -> Result<()> {
        log::info!("Starting monitoring system...");

        // Start health checking
        self.health_checker.lock().unwrap().start().await?;

        // Start metrics collection
        self.metrics_collector
            .lock()
            .unwrap()
            .start(self.config.metrics_interval)
            .await?;

        // Initialize structured logging
        self.logger.initialize()?;

        // Start telemetry collection if enabled
        if self.config.enable_profiling {
            self.telemetry.lock().unwrap().start().await?;
        }

        log::info!("Monitoring system started successfully");
        Ok(())
    }

    /// Stop the monitoring system
    #[allow(clippy::await_holding_lock)]
    pub async fn stop(&self) -> Result<()> {
        log::info!("Stopping monitoring system...");

        self.health_checker.lock().unwrap().stop().await?;
        self.metrics_collector.lock().unwrap().stop().await?;
        self.telemetry.lock().unwrap().stop().await?;

        log::info!("Monitoring system stopped");
        Ok(())
    }

    /// Record an operation for monitoring
    pub fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.metrics_collector
            .lock()
            .unwrap()
            .record_operation(operation, duration, success);

        if self.config.enable_tracing {
            self.logger.log_operation(operation, duration, success);
        }
    }

    /// Record a system metric
    pub fn record_metric(&self, name: &str, value: f64, tags: Option<&[(&str, &str)]>) {
        self.metrics_collector
            .lock()
            .unwrap()
            .record_metric(name, value, tags);
    }

    /// Get current system health status
    pub fn health_status(&self) -> health::HealthStatus {
        self.health_checker.lock().unwrap().current_status()
    }

    /// Get performance metrics summary
    pub fn metrics_summary(&self) -> metrics::MetricsSummary {
        self.metrics_collector.lock().unwrap().summary()
    }

    /// Check if system is healthy
    pub fn is_healthy(&self) -> bool {
        let health = self.health_status();
        let metrics = self.metrics_summary();

        // Check health status
        if !health.is_healthy() {
            return false;
        }

        // Check against alert thresholds
        if metrics.error_rate > self.config.alert_thresholds.max_error_rate {
            return false;
        }

        if metrics.avg_response_time_ms > self.config.alert_thresholds.max_response_time_ms {
            return false;
        }

        true
    }

    /// Get monitoring configuration
    pub fn config(&self) -> &MonitoringConfig {
        &self.config
    }

    /// Update alert thresholds
    pub fn update_thresholds(&mut self, thresholds: AlertThresholds) {
        log::info!("Updated alert thresholds: {thresholds:?}");
        self.config.alert_thresholds = thresholds;
    }

    /// Generate monitoring report
    pub fn generate_report(&self) -> MonitoringReport {
        let health = self.health_status();
        let metrics = self.metrics_summary();
        let telemetry_data = self.telemetry.lock().unwrap().summary();

        MonitoringReport {
            timestamp: Instant::now(),
            health,
            metrics,
            telemetry: telemetry_data,
            alerts: self.check_alerts(),
            is_healthy: self.is_healthy(),
        }
    }

    /// Check for alert conditions
    fn check_alerts(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let metrics = self.metrics_summary();
        let health = self.health_status();

        // Error rate alert
        if metrics.error_rate > self.config.alert_thresholds.max_error_rate {
            alerts.push(Alert {
                severity: AlertSeverity::High,
                message: format!(
                    "Error rate {:.2}% exceeds threshold of {:.2}%",
                    metrics.error_rate * 100.0,
                    self.config.alert_thresholds.max_error_rate * 100.0
                ),
                metric: "error_rate".to_string(),
                value: metrics.error_rate,
                threshold: self.config.alert_thresholds.max_error_rate,
            });
        }

        // Response time alert
        if metrics.avg_response_time_ms > self.config.alert_thresholds.max_response_time_ms {
            alerts.push(Alert {
                severity: AlertSeverity::Medium,
                message: format!(
                    "Average response time {}ms exceeds threshold of {}ms",
                    metrics.avg_response_time_ms, self.config.alert_thresholds.max_response_time_ms
                ),
                metric: "response_time".to_string(),
                value: metrics.avg_response_time_ms as f64,
                threshold: self.config.alert_thresholds.max_response_time_ms as f64,
            });
        }

        // Memory usage alert
        if let Some(memory_mb) = health.memory_usage_mb {
            if memory_mb > self.config.alert_thresholds.max_memory_mb {
                alerts.push(Alert {
                    severity: AlertSeverity::High,
                    message: format!(
                        "Memory usage {}MB exceeds threshold of {}MB",
                        memory_mb, self.config.alert_thresholds.max_memory_mb
                    ),
                    metric: "memory_usage".to_string(),
                    value: memory_mb as f64,
                    threshold: self.config.alert_thresholds.max_memory_mb as f64,
                });
            }
        }

        // CPU usage alert
        if let Some(cpu_percentage) = health.cpu_usage_percentage {
            if cpu_percentage > self.config.alert_thresholds.max_cpu_percentage {
                alerts.push(Alert {
                    severity: AlertSeverity::Medium,
                    message: format!(
                        "CPU usage {:.1}% exceeds threshold of {:.1}%",
                        cpu_percentage, self.config.alert_thresholds.max_cpu_percentage
                    ),
                    metric: "cpu_usage".to_string(),
                    value: cpu_percentage,
                    threshold: self.config.alert_thresholds.max_cpu_percentage,
                });
            }
        }

        alerts
    }
}

/// Comprehensive monitoring report
#[derive(Debug, Clone)]
pub struct MonitoringReport {
    pub timestamp: Instant,
    pub health: health::HealthStatus,
    pub metrics: metrics::MetricsSummary,
    pub telemetry: telemetry::TelemetrySummary,
    pub alerts: Vec<Alert>,
    pub is_healthy: bool,
}

/// Alert information
#[derive(Debug, Clone)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub message: String,
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitoring_config_default() {
        let config = MonitoringConfig::default();
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.metrics_interval, Duration::from_secs(60));
        assert!(config.enable_profiling);
        assert!(!config.enable_tracing);
        assert_eq!(config.metrics_retention_limit, 10000);
    }

    #[test]
    fn test_alert_thresholds_default() {
        let thresholds = AlertThresholds::default();
        assert_eq!(thresholds.max_error_rate, 0.05);
        assert_eq!(thresholds.max_response_time_ms, 5000);
        assert_eq!(thresholds.max_memory_mb, 1024);
        assert_eq!(thresholds.max_cpu_percentage, 80.0);
    }

    #[tokio::test]
    async fn test_monitoring_system_creation() {
        let monitoring = MonitoringSystem::new();
        assert!(monitoring.is_ok());

        let monitoring = monitoring.unwrap();
        assert!(monitoring.is_healthy()); // Should start healthy
    }

    #[tokio::test]
    async fn test_monitoring_system_with_config() {
        let config = MonitoringConfig {
            health_check_interval: Duration::from_secs(10),
            metrics_interval: Duration::from_secs(30),
            enable_profiling: false,
            enable_tracing: true,
            metrics_retention_limit: 5000,
            alert_thresholds: AlertThresholds {
                max_error_rate: 0.1,
                max_response_time_ms: 10000,
                max_memory_mb: 2048,
                max_cpu_percentage: 90.0,
            },
        };

        let monitoring = MonitoringSystem::with_config(config.clone());
        assert!(monitoring.is_ok());

        let monitoring = monitoring.unwrap();
        assert_eq!(
            monitoring.config().health_check_interval,
            Duration::from_secs(10)
        );
        assert_eq!(
            monitoring.config().metrics_interval,
            Duration::from_secs(30)
        );
        assert!(!monitoring.config().enable_profiling);
        assert!(monitoring.config().enable_tracing);
    }

    #[test]
    fn test_operation_recording() {
        let monitoring = MonitoringSystem::new().unwrap();

        // Record some successful operations
        monitoring.record_operation("embed", Duration::from_millis(100), true);
        monitoring.record_operation("extract", Duration::from_millis(50), true);
        monitoring.record_operation("embed", Duration::from_millis(200), false);

        let summary = monitoring.metrics_summary();
        assert!(summary.total_operations > 0);
    }

    #[test]
    fn test_metric_recording() {
        let monitoring = MonitoringSystem::new().unwrap();

        // Record some metrics
        monitoring.record_metric("throughput", 1024.0, Some(&[("unit", "bytes_per_sec")]));
        monitoring.record_metric("latency", 50.0, Some(&[("unit", "milliseconds")]));

        let summary = monitoring.metrics_summary();
        assert!(!summary.custom_metrics.is_empty());
    }

    #[test]
    fn test_alert_generation() {
        let mut monitoring = MonitoringSystem::new().unwrap();

        // Set very strict thresholds to trigger alerts
        monitoring.update_thresholds(AlertThresholds {
            max_error_rate: 0.01,     // 1%
            max_response_time_ms: 10, // 10ms
            max_memory_mb: 1,         // 1MB
            max_cpu_percentage: 1.0,  // 1%
        });

        // Record some operations that should trigger alerts
        monitoring.record_operation("test", Duration::from_millis(100), false);
        monitoring.record_operation("test", Duration::from_millis(200), false);

        let report = monitoring.generate_report();
        // Should have alerts due to high error rate and response time
        assert!(!report.alerts.is_empty());
        assert!(!report.is_healthy);
    }

    #[test]
    fn test_alert_severity_string() {
        assert_eq!(AlertSeverity::Low.as_str(), "low");
        assert_eq!(AlertSeverity::Medium.as_str(), "medium");
        assert_eq!(AlertSeverity::High.as_str(), "high");
        assert_eq!(AlertSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_monitoring_report_generation() {
        let monitoring = MonitoringSystem::new().unwrap();

        // Record some data
        monitoring.record_operation("test", Duration::from_millis(50), true);
        monitoring.record_metric("custom", 42.0, None);

        let report = monitoring.generate_report();
        assert!(report.is_healthy);
        assert!(report.alerts.is_empty());
        assert!(report.metrics.total_operations > 0);
    }
}
