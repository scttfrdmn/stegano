//! Metrics collection and analysis for performance monitoring

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

/// Metrics collector for system performance monitoring
#[derive(Debug)]
pub struct MetricsCollector {
    metrics: Arc<RwLock<MetricsData>>,
    retention_limit: usize,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Internal metrics storage
#[derive(Debug)]
struct MetricsData {
    operations: VecDeque<OperationMetric>,
    custom_metrics: HashMap<String, VecDeque<CustomMetric>>,
    counters: HashMap<String, u64>,
    gauges: HashMap<String, f64>,
    histograms: HashMap<String, Vec<f64>>,
}

/// Metrics for individual operations
#[derive(Debug, Clone)]
pub struct OperationMetric {
    pub operation: String,
    pub duration: Duration,
    pub success: bool,
    pub timestamp: Instant,
    pub tags: HashMap<String, String>,
}

/// Custom metric data point
#[derive(Debug, Clone)]
pub struct CustomMetric {
    pub name: String,
    pub value: f64,
    pub timestamp: Instant,
    pub tags: HashMap<String, String>,
}

/// Summary of collected metrics
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub error_rate: f64,
    pub avg_response_time_ms: u64,
    pub min_response_time_ms: u64,
    pub max_response_time_ms: u64,
    pub p95_response_time_ms: u64,
    pub p99_response_time_ms: u64,
    pub operations_per_second: f64,
    pub custom_metrics: HashMap<String, MetricStatistics>,
    pub top_operations: Vec<OperationStats>,
}

/// Statistics for a specific metric
#[derive(Debug, Clone)]
pub struct MetricStatistics {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub sum: f64,
    pub latest_value: f64,
}

/// Statistics for specific operations
#[derive(Debug, Clone)]
pub struct OperationStats {
    pub operation: String,
    pub count: u64,
    pub avg_duration_ms: u64,
    pub success_rate: f64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(retention_limit: usize) -> crate::core::Result<Self> {
        let metrics = Arc::new(RwLock::new(MetricsData::new()));

        Ok(Self {
            metrics,
            retention_limit,
            shutdown_tx: None,
        })
    }

    /// Start periodic metrics processing
    pub async fn start(&mut self, collection_interval: Duration) -> crate::core::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let metrics = Arc::clone(&self.metrics);
        let retention_limit = self.retention_limit;

        tokio::spawn(async move {
            let mut interval = interval(collection_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::process_metrics(&metrics, retention_limit);
                    }
                    _ = shutdown_rx.recv() => {
                        log::info!("Metrics collector shutting down");
                        break;
                    }
                }
            }
        });

        log::info!("Metrics collector started with interval: {collection_interval:?}");
        Ok(())
    }

    /// Stop metrics collection
    pub async fn stop(&self) -> crate::core::Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    /// Record an operation
    pub fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.record_operation_with_tags(operation, duration, success, &[]);
    }

    /// Record an operation with tags
    pub fn record_operation_with_tags(
        &self,
        operation: &str,
        duration: Duration,
        success: bool,
        tags: &[(&str, &str)],
    ) {
        let metric = OperationMetric {
            operation: operation.to_string(),
            duration,
            success,
            timestamp: Instant::now(),
            tags: tags
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };

        let mut metrics = self.metrics.write().unwrap();
        metrics.operations.push_back(metric);

        // Update counters
        let total_key = format!("operations.{operation}.total");
        *metrics.counters.entry(total_key).or_insert(0) += 1;

        if success {
            let success_key = format!("operations.{operation}.success");
            *metrics.counters.entry(success_key).or_insert(0) += 1;
        } else {
            let error_key = format!("operations.{operation}.error");
            *metrics.counters.entry(error_key).or_insert(0) += 1;
        }

        // Update histograms
        let duration_key = format!("operations.{operation}.duration_ms");
        metrics
            .histograms
            .entry(duration_key)
            .or_default()
            .push(duration.as_millis() as f64);
    }

    /// Record a custom metric
    pub fn record_metric(&self, name: &str, value: f64, tags: Option<&[(&str, &str)]>) {
        let metric = CustomMetric {
            name: name.to_string(),
            value,
            timestamp: Instant::now(),
            tags: tags
                .map(|t| {
                    t.iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        };

        let mut metrics = self.metrics.write().unwrap();
        metrics
            .custom_metrics
            .entry(name.to_string())
            .or_default()
            .push_back(metric);

        // Update gauge
        metrics.gauges.insert(name.to_string(), value);
    }

    /// Increment a counter
    pub fn increment_counter(&self, name: &str) {
        self.add_to_counter(name, 1);
    }

    /// Add to a counter
    pub fn add_to_counter(&self, name: &str, value: u64) {
        let mut metrics = self.metrics.write().unwrap();
        *metrics.counters.entry(name.to_string()).or_insert(0) += value;
    }

    /// Set a gauge value
    pub fn set_gauge(&self, name: &str, value: f64) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.gauges.insert(name.to_string(), value);
    }

    /// Get metrics summary
    pub fn summary(&self) -> MetricsSummary {
        let metrics = self.metrics.read().unwrap();
        self.calculate_summary(&metrics)
    }

    /// Get current counter value
    pub fn get_counter(&self, name: &str) -> u64 {
        let metrics = self.metrics.read().unwrap();
        metrics.counters.get(name).copied().unwrap_or(0)
    }

    /// Get current gauge value
    pub fn get_gauge(&self, name: &str) -> Option<f64> {
        let metrics = self.metrics.read().unwrap();
        metrics.gauges.get(name).copied()
    }

    /// Process and clean up metrics
    fn process_metrics(metrics: &Arc<RwLock<MetricsData>>, retention_limit: usize) {
        let mut metrics = metrics.write().unwrap();

        // Trim operations to retention limit
        while metrics.operations.len() > retention_limit {
            metrics.operations.pop_front();
        }

        // Trim custom metrics
        for (_, metric_queue) in metrics.custom_metrics.iter_mut() {
            while metric_queue.len() > retention_limit {
                metric_queue.pop_front();
            }
        }

        // Trim histograms
        for (_, histogram) in metrics.histograms.iter_mut() {
            if histogram.len() > retention_limit {
                histogram.drain(0..histogram.len() - retention_limit);
            }
        }
    }

    /// Calculate metrics summary
    fn calculate_summary(&self, metrics: &MetricsData) -> MetricsSummary {
        let total_operations = metrics.operations.len() as u64;
        let successful_operations =
            metrics.operations.iter().filter(|op| op.success).count() as u64;
        let failed_operations = total_operations - successful_operations;

        let error_rate = if total_operations > 0 {
            failed_operations as f64 / total_operations as f64
        } else {
            0.0
        };

        // Calculate response time statistics
        let mut durations: Vec<u64> = metrics
            .operations
            .iter()
            .map(|op| op.duration.as_millis() as u64)
            .collect();
        durations.sort();

        let avg_response_time_ms = if !durations.is_empty() {
            durations.iter().sum::<u64>() / durations.len() as u64
        } else {
            0
        };

        let min_response_time_ms = durations.first().copied().unwrap_or(0);
        let max_response_time_ms = durations.last().copied().unwrap_or(0);

        let p95_response_time_ms = if !durations.is_empty() {
            let index = ((durations.len() as f64) * 0.95) as usize;
            durations
                .get(index.min(durations.len() - 1))
                .copied()
                .unwrap_or(0)
        } else {
            0
        };

        let p99_response_time_ms = if !durations.is_empty() {
            let index = ((durations.len() as f64) * 0.99) as usize;
            durations
                .get(index.min(durations.len() - 1))
                .copied()
                .unwrap_or(0)
        } else {
            0
        };

        // Calculate operations per second
        let operations_per_second = if !metrics.operations.is_empty() {
            let oldest = metrics.operations.front().unwrap().timestamp;
            let newest = metrics.operations.back().unwrap().timestamp;
            let time_span = newest.duration_since(oldest);

            if time_span.as_secs() > 0 {
                total_operations as f64 / time_span.as_secs_f64()
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Calculate custom metrics statistics
        let custom_metrics = self.calculate_custom_metrics_stats(metrics);

        // Calculate top operations
        let top_operations = self.calculate_top_operations(metrics);

        MetricsSummary {
            total_operations,
            successful_operations,
            failed_operations,
            error_rate,
            avg_response_time_ms,
            min_response_time_ms,
            max_response_time_ms,
            p95_response_time_ms,
            p99_response_time_ms,
            operations_per_second,
            custom_metrics,
            top_operations,
        }
    }

    /// Calculate statistics for custom metrics
    fn calculate_custom_metrics_stats(
        &self,
        metrics: &MetricsData,
    ) -> HashMap<String, MetricStatistics> {
        let mut stats = HashMap::new();

        for (name, metric_queue) in &metrics.custom_metrics {
            if metric_queue.is_empty() {
                continue;
            }

            let values: Vec<f64> = metric_queue.iter().map(|m| m.value).collect();
            let count = values.len() as u64;
            let sum = values.iter().sum::<f64>();
            let avg = sum / count as f64;
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let latest_value = metric_queue.back().unwrap().value;

            stats.insert(
                name.clone(),
                MetricStatistics {
                    count,
                    min,
                    max,
                    avg,
                    sum,
                    latest_value,
                },
            );
        }

        stats
    }

    /// Calculate top operations by frequency
    fn calculate_top_operations(&self, metrics: &MetricsData) -> Vec<OperationStats> {
        let mut operation_stats: HashMap<String, (u64, Vec<Duration>, u64)> = HashMap::new();

        for op in &metrics.operations {
            let entry = operation_stats
                .entry(op.operation.clone())
                .or_insert((0, Vec::new(), 0));
            entry.0 += 1; // count
            entry.1.push(op.duration); // durations
            if op.success {
                entry.2 += 1; // success count
            }
        }

        let mut top_ops: Vec<OperationStats> = operation_stats
            .into_iter()
            .map(|(operation, (count, durations, success_count))| {
                let avg_duration_ms = if !durations.is_empty() {
                    durations.iter().map(|d| d.as_millis()).sum::<u128>() / durations.len() as u128
                } else {
                    0
                } as u64;

                let success_rate = if count > 0 {
                    success_count as f64 / count as f64
                } else {
                    0.0
                };

                OperationStats {
                    operation,
                    count,
                    avg_duration_ms,
                    success_rate,
                }
            })
            .collect();

        // Sort by count (most frequent first)
        top_ops.sort_by(|a, b| b.count.cmp(&a.count));

        // Take top 10
        top_ops.truncate(10);
        top_ops
    }
}

impl MetricsData {
    fn new() -> Self {
        Self {
            operations: VecDeque::new(),
            custom_metrics: HashMap::new(),
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new(1000);
        assert!(collector.is_ok());

        let collector = collector.unwrap();
        let summary = collector.summary();
        assert_eq!(summary.total_operations, 0);
    }

    #[test]
    fn test_operation_recording() {
        let collector = MetricsCollector::new(1000).unwrap();

        collector.record_operation("test_op", Duration::from_millis(100), true);
        collector.record_operation("test_op", Duration::from_millis(200), false);
        collector.record_operation("other_op", Duration::from_millis(50), true);

        let summary = collector.summary();
        assert_eq!(summary.total_operations, 3);
        assert_eq!(summary.successful_operations, 2);
        assert_eq!(summary.failed_operations, 1);
        assert_eq!(summary.error_rate, 1.0 / 3.0);
    }

    #[test]
    fn test_custom_metrics() {
        let collector = MetricsCollector::new(1000).unwrap();

        collector.record_metric("throughput", 1024.0, Some(&[("unit", "bytes")]));
        collector.record_metric("throughput", 2048.0, Some(&[("unit", "bytes")]));
        collector.record_metric("latency", 50.0, None);

        let summary = collector.summary();
        assert!(summary.custom_metrics.contains_key("throughput"));
        assert!(summary.custom_metrics.contains_key("latency"));

        let throughput_stats = &summary.custom_metrics["throughput"];
        assert_eq!(throughput_stats.count, 2);
        assert_eq!(throughput_stats.min, 1024.0);
        assert_eq!(throughput_stats.max, 2048.0);
        assert_eq!(throughput_stats.avg, 1536.0);
    }

    #[test]
    fn test_counters_and_gauges() {
        let collector = MetricsCollector::new(1000).unwrap();

        collector.increment_counter("requests");
        collector.add_to_counter("requests", 5);
        collector.set_gauge("cpu_usage", 75.5);

        assert_eq!(collector.get_counter("requests"), 6);
        assert_eq!(collector.get_gauge("cpu_usage"), Some(75.5));
        assert_eq!(collector.get_gauge("nonexistent"), None);
    }

    #[test]
    fn test_response_time_percentiles() {
        let collector = MetricsCollector::new(1000).unwrap();

        // Record operations with various durations
        for i in 1..=100 {
            collector.record_operation("test", Duration::from_millis(i * 10), true);
        }

        let summary = collector.summary();
        assert_eq!(summary.total_operations, 100);
        assert!(summary.p95_response_time_ms > summary.avg_response_time_ms);
        assert!(summary.p99_response_time_ms > summary.p95_response_time_ms);
        assert!(summary.max_response_time_ms >= summary.p99_response_time_ms);
    }

    #[test]
    fn test_top_operations() {
        let collector = MetricsCollector::new(1000).unwrap();

        // Record different operations with different frequencies
        for _ in 0..10 {
            collector.record_operation("frequent_op", Duration::from_millis(50), true);
        }
        for _ in 0..5 {
            collector.record_operation("medium_op", Duration::from_millis(100), true);
        }
        for _ in 0..2 {
            collector.record_operation("rare_op", Duration::from_millis(200), false);
        }

        let summary = collector.summary();
        assert!(!summary.top_operations.is_empty());

        // Most frequent should be first
        assert_eq!(summary.top_operations[0].operation, "frequent_op");
        assert_eq!(summary.top_operations[0].count, 10);
        assert_eq!(summary.top_operations[0].success_rate, 1.0);

        assert_eq!(summary.top_operations[1].operation, "medium_op");
        assert_eq!(summary.top_operations[1].count, 5);

        assert_eq!(summary.top_operations[2].operation, "rare_op");
        assert_eq!(summary.top_operations[2].count, 2);
        assert_eq!(summary.top_operations[2].success_rate, 0.0);
    }

    #[test]
    fn test_operations_per_second() {
        let collector = MetricsCollector::new(1000).unwrap();

        // Record an operation, wait a bit, then record another
        collector.record_operation("test", Duration::from_millis(10), true);

        // Simulate some time passing by manually modifying timestamps
        // In a real scenario, this would be measured over actual time
        let summary = collector.summary();

        // With only one operation, ops/sec calculation might be 0 or undefined
        // This test mainly ensures the calculation doesn't panic
        assert!(summary.operations_per_second >= 0.0);
    }

    #[tokio::test]
    async fn test_metrics_processing() {
        let collector = MetricsCollector::new(5).unwrap(); // Small retention limit

        // Record more operations than the retention limit
        for i in 0..10 {
            collector.record_operation("test", Duration::from_millis(i * 10), true);
        }

        // Process metrics to trigger cleanup
        {
            let metrics = collector.metrics.clone();
            MetricsCollector::process_metrics(&metrics, 5);
        }

        let summary = collector.summary();
        assert!(summary.total_operations <= 5); // Should be trimmed to retention limit
    }
}
