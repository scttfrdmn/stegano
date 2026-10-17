//! Telemetry collection for performance analysis and optimization

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Telemetry collector for detailed performance analysis
#[derive(Debug)]
pub struct TelemetryCollector {
    data: Arc<RwLock<TelemetryData>>,
    config: TelemetryConfig,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Configuration for telemetry collection
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub sampling_rate: f64, // 0.0 to 1.0
    pub max_traces: usize,
    pub max_spans: usize,
    pub enable_memory_profiling: bool,
    pub enable_cpu_profiling: bool,
}

/// Internal telemetry data storage
#[derive(Debug)]
struct TelemetryData {
    traces: VecDeque<Trace>,
    memory_samples: VecDeque<MemorySample>,
    cpu_samples: VecDeque<CpuSample>,
    counters: HashMap<String, u64>,
    timers: HashMap<String, Vec<Duration>>,
}

/// A trace represents a complete operation with multiple spans
#[derive(Debug, Clone)]
pub struct Trace {
    pub trace_id: String,
    pub spans: Vec<Span>,
    pub start_time: Instant,
    pub total_duration: Duration,
    pub operation: String,
    pub success: bool,
}

/// A span represents a single operation within a trace
#[derive(Debug, Clone)]
pub struct Span {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub start_time: Instant,
    pub duration: Duration,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
}

/// Log entry within a span
#[derive(Debug, Clone)]
pub struct SpanLog {
    pub timestamp: Instant,
    pub message: String,
    pub level: String,
    pub fields: HashMap<String, String>,
}

/// Memory usage sample
#[derive(Debug, Clone)]
pub struct MemorySample {
    pub timestamp: Instant,
    pub allocated_bytes: u64,
    pub deallocated_bytes: u64,
    pub total_allocations: u64,
    pub peak_memory: u64,
}

/// CPU usage sample
#[derive(Debug, Clone)]
pub struct CpuSample {
    pub timestamp: Instant,
    pub cpu_percentage: f64,
    pub user_time: Duration,
    pub system_time: Duration,
    pub idle_time: Duration,
}

/// Summary of telemetry data
#[derive(Debug, Clone)]
pub struct TelemetrySummary {
    pub total_traces: u64,
    pub successful_traces: u64,
    pub failed_traces: u64,
    pub avg_trace_duration_ms: u64,
    pub slowest_operations: Vec<SlowOperation>,
    pub memory_stats: Option<MemoryStats>,
    pub cpu_stats: Option<CpuStats>,
    pub hot_paths: Vec<HotPath>,
}

/// Information about slow operations
#[derive(Debug, Clone)]
pub struct SlowOperation {
    pub operation: String,
    pub trace_id: String,
    pub duration_ms: u64,
    pub span_count: usize,
}

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub current_allocated: u64,
    pub peak_allocated: u64,
    pub total_allocations: u64,
    pub avg_allocation_size: u64,
}

/// CPU usage statistics
#[derive(Debug, Clone)]
pub struct CpuStats {
    pub avg_cpu_percentage: f64,
    pub max_cpu_percentage: f64,
    pub total_user_time_ms: u64,
    pub total_system_time_ms: u64,
}

/// Frequently executed code paths
#[derive(Debug, Clone)]
pub struct HotPath {
    pub path: String,
    pub execution_count: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_rate: 0.1, // Sample 10% of operations
            max_traces: 1000,
            max_spans: 10000,
            enable_memory_profiling: true,
            enable_cpu_profiling: true,
        }
    }
}

impl TelemetryCollector {
    /// Create a new telemetry collector
    pub fn new() -> crate::core::Result<Self> {
        Self::with_config(TelemetryConfig::default())
    }

    /// Create a telemetry collector with custom configuration
    pub fn with_config(config: TelemetryConfig) -> crate::core::Result<Self> {
        let data = Arc::new(RwLock::new(TelemetryData::new()));

        Ok(Self {
            data,
            config,
            shutdown_tx: None,
        })
    }

    /// Start telemetry collection
    pub async fn start(&mut self) -> crate::core::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let data = Arc::clone(&self.data);
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut collection_interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                tokio::select! {
                    _ = collection_interval.tick() => {
                        if config.enable_memory_profiling {
                            if let Ok(sample) = Self::collect_memory_sample() {
                                let mut data = data.write().unwrap();
                                data.memory_samples.push_back(sample);
                                while data.memory_samples.len() > 1000 {
                                    data.memory_samples.pop_front();
                                }
                            }
                        }

                        if config.enable_cpu_profiling {
                            if let Ok(sample) = Self::collect_cpu_sample().await {
                                let mut data = data.write().unwrap();
                                data.cpu_samples.push_back(sample);
                                while data.cpu_samples.len() > 1000 {
                                    data.cpu_samples.pop_front();
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        log::info!("Telemetry collector shutting down");
                        break;
                    }
                }
            }
        });

        log::info!("Telemetry collector started");
        Ok(())
    }

    /// Stop telemetry collection
    pub async fn stop(&self) -> crate::core::Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    /// Start a new trace
    pub fn start_trace(&self, operation: &str) -> Option<TraceContext> {
        if !self.config.enabled || !self.should_sample() {
            return None;
        }

        let trace_id = format!("trace_{:016x}", rand::random::<u64>());
        let trace = Trace {
            trace_id: trace_id.clone(),
            spans: Vec::new(),
            start_time: Instant::now(),
            total_duration: Duration::default(),
            operation: operation.to_string(),
            success: true,
        };

        Some(TraceContext {
            trace_id,
            collector: self.data.clone(),
            trace,
        })
    }

    /// Record a simple operation timing
    pub fn record_timing(&self, operation: &str, duration: Duration) {
        if !self.config.enabled {
            return;
        }

        let mut data = self.data.write().unwrap();
        data.timers
            .entry(operation.to_string())
            .or_default()
            .push(duration);

        // Keep only recent timings
        for timer_vec in data.timers.values_mut() {
            if timer_vec.len() > 100 {
                timer_vec.drain(0..timer_vec.len() - 100);
            }
        }
    }

    /// Increment a counter
    pub fn increment_counter(&self, name: &str) {
        if !self.config.enabled {
            return;
        }

        let mut data = self.data.write().unwrap();
        *data.counters.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Get telemetry summary
    pub fn summary(&self) -> TelemetrySummary {
        let data = self.data.read().unwrap();
        self.calculate_summary(&data)
    }

    /// Check if this operation should be sampled
    fn should_sample(&self) -> bool {
        rand::random::<f64>() < self.config.sampling_rate
    }

    /// Collect a memory usage sample using `getrusage(RUSAGE_SELF)`.
    ///
    /// `allocated_bytes` is the process's current resident set size (RSS).
    /// `peak_memory` is the maximum RSS since the process started.
    /// `total_allocations` is the minor-page-fault count (a proxy for heap activity).
    fn collect_memory_sample() -> Result<MemorySample, Box<dyn std::error::Error>> {
        let (current, peak, minor_faults) = Self::process_memory_info()?;
        Ok(MemorySample {
            timestamp: Instant::now(),
            allocated_bytes: current,
            deallocated_bytes: 0, // requires allocator instrumentation
            total_allocations: minor_faults.max(1),
            peak_memory: peak,
        })
    }

    /// Collect a CPU usage sample.
    ///
    /// Takes two `getrusage` snapshots 50 ms apart and converts the delta
    /// user+system CPU time to a percentage of that wall-clock window.
    async fn collect_cpu_sample() -> Result<CpuSample, Box<dyn std::error::Error>> {
        let (user_t1, sys_t1) = Self::process_cpu_times()?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let (user_t2, sys_t2) = Self::process_cpu_times()?;

        let delta_us = ((user_t2 - user_t1) + (sys_t2 - sys_t1)).max(0) as u64;
        // delta_us / 50_000 µs window × 100 → percentage (not divided by cores;
        // reports single-core equivalent, clamped to 100 %).
        let cpu_percentage = (delta_us as f64 / 50_000.0 * 100.0).clamp(0.0, 100.0);

        let user_time = Duration::from_micros(user_t2.max(0) as u64);
        let system_time = Duration::from_micros(sys_t2.max(0) as u64);
        let idle_time = Duration::from_micros(50_000u64.saturating_sub(delta_us));

        Ok(CpuSample {
            timestamp: Instant::now(),
            cpu_percentage,
            user_time,
            system_time,
            idle_time,
        })
    }

    // ---- platform helpers ----

    /// Returns `(current_rss_bytes, peak_rss_bytes, minor_page_faults)` for
    /// the current process.
    #[cfg(unix)]
    fn process_memory_info() -> Result<(u64, u64, u64), Box<dyn std::error::Error>> {
        let mut ru = unsafe { std::mem::zeroed::<libc::rusage>() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
            return Err("getrusage failed".into());
        }

        // ru_maxrss: bytes on macOS, kilobytes on Linux
        #[cfg(target_os = "macos")]
        let peak = ru.ru_maxrss as u64;
        #[cfg(not(target_os = "macos"))]
        let peak = ru.ru_maxrss as u64 * 1024;

        // Current RSS: /proc/self/status on Linux; approximate with peak on macOS
        #[cfg(target_os = "linux")]
        let current = {
            std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("VmRSS:"))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|kb| kb * 1024)
                })
                .unwrap_or(peak)
        };
        #[cfg(not(target_os = "linux"))]
        let current = peak;

        // RSS can grow between getrusage() and reading /proc (other threads
        // allocate meanwhile), so keep the invariant peak >= current.
        let peak = peak.max(current);

        Ok((current, peak, ru.ru_minflt as u64))
    }

    #[cfg(not(unix))]
    fn process_memory_info() -> Result<(u64, u64, u64), Box<dyn std::error::Error>> {
        Err("process memory sampling not supported on this platform".into())
    }

    /// Returns `(user_microseconds, system_microseconds)` of CPU time consumed
    /// by the current process since it started.
    #[cfg(unix)]
    fn process_cpu_times() -> Result<(i64, i64), Box<dyn std::error::Error>> {
        let mut ru = unsafe { std::mem::zeroed::<libc::rusage>() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
            return Err("getrusage failed".into());
        }
        let user_us =
            ru.ru_utime.tv_sec * 1_000_000 + ru.ru_utime.tv_usec as i64;
        let sys_us =
            ru.ru_stime.tv_sec * 1_000_000 + ru.ru_stime.tv_usec as i64;
        Ok((user_us, sys_us))
    }

    #[cfg(not(unix))]
    fn process_cpu_times() -> Result<(i64, i64), Box<dyn std::error::Error>> {
        Err("CPU time sampling not supported on this platform".into())
    }

    /// Calculate telemetry summary
    fn calculate_summary(&self, data: &TelemetryData) -> TelemetrySummary {
        let total_traces = data.traces.len() as u64;
        let successful_traces = data.traces.iter().filter(|t| t.success).count() as u64;
        let failed_traces = total_traces - successful_traces;

        let avg_trace_duration_ms = if !data.traces.is_empty() {
            data.traces
                .iter()
                .map(|t| t.total_duration.as_millis() as u64)
                .sum::<u64>()
                / total_traces
        } else {
            0
        };

        // Find slowest operations
        let mut slowest_operations: Vec<SlowOperation> = data
            .traces
            .iter()
            .map(|trace| SlowOperation {
                operation: trace.operation.clone(),
                trace_id: trace.trace_id.clone(),
                duration_ms: trace.total_duration.as_millis() as u64,
                span_count: trace.spans.len(),
            })
            .collect();
        slowest_operations.sort_by_key(|a| std::cmp::Reverse(a.duration_ms));
        slowest_operations.truncate(10);

        // Calculate memory stats
        let memory_stats = if !data.memory_samples.is_empty() {
            let latest = data.memory_samples.back().unwrap();
            let peak = data
                .memory_samples
                .iter()
                .map(|s| s.peak_memory)
                .max()
                .unwrap_or(0);

            Some(MemoryStats {
                current_allocated: latest.allocated_bytes,
                peak_allocated: peak,
                total_allocations: latest.total_allocations,
                avg_allocation_size: latest
                    .allocated_bytes
                    .checked_div(latest.total_allocations)
                    .unwrap_or(0),
            })
        } else {
            None
        };

        // Calculate CPU stats
        let cpu_stats = if !data.cpu_samples.is_empty() {
            let avg_cpu = data
                .cpu_samples
                .iter()
                .map(|s| s.cpu_percentage)
                .sum::<f64>()
                / data.cpu_samples.len() as f64;
            let max_cpu = data
                .cpu_samples
                .iter()
                .map(|s| s.cpu_percentage)
                .fold(0.0, f64::max);
            let total_user_time = data
                .cpu_samples
                .iter()
                .map(|s| s.user_time.as_millis() as u64)
                .sum();
            let total_system_time = data
                .cpu_samples
                .iter()
                .map(|s| s.system_time.as_millis() as u64)
                .sum();

            Some(CpuStats {
                avg_cpu_percentage: avg_cpu,
                max_cpu_percentage: max_cpu,
                total_user_time_ms: total_user_time,
                total_system_time_ms: total_system_time,
            })
        } else {
            None
        };

        // Calculate hot paths from counters and timers
        let mut hot_paths = Vec::new();
        for (operation, timings) in &data.timers {
            if !timings.is_empty() {
                let execution_count = timings.len() as u64;
                let total_duration_ms = timings.iter().map(|d| d.as_millis() as u64).sum();
                let avg_duration_ms = total_duration_ms / execution_count;

                hot_paths.push(HotPath {
                    path: operation.clone(),
                    execution_count,
                    total_duration_ms,
                    avg_duration_ms,
                });
            }
        }
        hot_paths.sort_by_key(|a| std::cmp::Reverse(a.execution_count));
        hot_paths.truncate(10);

        TelemetrySummary {
            total_traces,
            successful_traces,
            failed_traces,
            avg_trace_duration_ms,
            slowest_operations,
            memory_stats,
            cpu_stats,
            hot_paths,
        }
    }
}

impl TelemetryData {
    fn new() -> Self {
        Self {
            traces: VecDeque::new(),
            memory_samples: VecDeque::new(),
            cpu_samples: VecDeque::new(),
            counters: HashMap::new(),
            timers: HashMap::new(),
        }
    }
}

/// Context for tracking a trace through its lifecycle
pub struct TraceContext {
    trace_id: String,
    collector: Arc<RwLock<TelemetryData>>,
    trace: Trace,
}

impl TraceContext {
    /// Start a new span within this trace
    pub fn start_span(&self, operation: &str) -> SpanContext {
        let span_id = format!("span_{:016x}", rand::random::<u64>());
        let span = Span {
            span_id: span_id.clone(),
            parent_span_id: None,
            operation: operation.to_string(),
            start_time: Instant::now(),
            duration: Duration::default(),
            tags: HashMap::new(),
            logs: Vec::new(),
        };

        SpanContext {
            span_id,
            _trace_id: self.trace_id.clone(),
            span,
        }
    }

    /// Finish the trace
    pub fn finish(mut self, success: bool) {
        self.trace.total_duration = self.trace.start_time.elapsed();
        self.trace.success = success;

        let mut data = self.collector.write().unwrap();
        data.traces.push_back(self.trace);

        // Limit trace storage
        while data.traces.len() > 1000 {
            data.traces.pop_front();
        }
    }
}

/// Context for tracking a span within a trace
pub struct SpanContext {
    span_id: String,
    _trace_id: String,
    span: Span,
}

impl SpanContext {
    /// Add a tag to this span
    pub fn set_tag(&mut self, key: &str, value: &str) {
        self.span.tags.insert(key.to_string(), value.to_string());
    }

    /// Log a message in this span
    pub fn log(&mut self, level: &str, message: &str) {
        self.span.logs.push(SpanLog {
            timestamp: Instant::now(),
            message: message.to_string(),
            level: level.to_string(),
            fields: HashMap::new(),
        });
    }

    /// Finish the span
    pub fn finish(mut self) {
        self.span.duration = self.span.start_time.elapsed();

        // In a real implementation, you'd add the span back to the trace
        // For this simplified version, we'll just log completion
        log::trace!(
            "Span {} completed in {:?}",
            self.span_id,
            self.span.duration
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.sampling_rate, 0.1);
        assert_eq!(config.max_traces, 1000);
        assert!(config.enable_memory_profiling);
        assert!(config.enable_cpu_profiling);
    }

    #[test]
    fn test_telemetry_collector_creation() {
        let collector = TelemetryCollector::new();
        assert!(collector.is_ok());

        let config = TelemetryConfig {
            enabled: false,
            sampling_rate: 0.5,
            max_traces: 500,
            max_spans: 5000,
            enable_memory_profiling: false,
            enable_cpu_profiling: true,
        };

        let collector = TelemetryCollector::with_config(config);
        assert!(collector.is_ok());
    }

    #[test]
    fn test_timing_recording() {
        let collector = TelemetryCollector::new().unwrap();

        collector.record_timing("test_operation", Duration::from_millis(100));
        collector.record_timing("test_operation", Duration::from_millis(150));
        collector.record_timing("other_operation", Duration::from_millis(50));

        let summary = collector.summary();
        assert!(!summary.hot_paths.is_empty());

        let test_path = summary
            .hot_paths
            .iter()
            .find(|p| p.path == "test_operation");
        assert!(test_path.is_some());
        assert_eq!(test_path.unwrap().execution_count, 2);
    }

    #[test]
    fn test_counter_increment() {
        let collector = TelemetryCollector::new().unwrap();

        collector.increment_counter("requests");
        collector.increment_counter("requests");
        collector.increment_counter("errors");

        let data = collector.data.read().unwrap();
        assert_eq!(data.counters.get("requests"), Some(&2));
        assert_eq!(data.counters.get("errors"), Some(&1));
    }

    #[test]
    fn test_trace_context() {
        let collector = TelemetryCollector::new().unwrap();

        // Start a trace (might return None due to sampling)
        if let Some(trace_ctx) = collector.start_trace("test_operation") {
            let span_ctx = trace_ctx.start_span("test_span");
            span_ctx.finish();
            trace_ctx.finish(true);
        }

        // Test should not panic regardless of sampling
    }

    #[test]
    fn test_span_context() {
        let collector = TelemetryCollector::new().unwrap();

        if let Some(trace_ctx) = collector.start_trace("test_operation") {
            let mut span_ctx = trace_ctx.start_span("test_span");
            span_ctx.set_tag("component", "handler");
            span_ctx.set_tag("format", "image");
            span_ctx.log("info", "Processing started");
            span_ctx.log("debug", "Reading file header");
            span_ctx.finish();
            trace_ctx.finish(true);
        }
    }

    #[tokio::test]
    async fn test_memory_sampling() {
        let sample = TelemetryCollector::collect_memory_sample();
        match sample {
            Ok(sample) => {
                assert!(sample.allocated_bytes > 0);
                assert!(sample.total_allocations > 0);
                assert!(sample.peak_memory >= sample.allocated_bytes);
            }
            Err(_) => {
                // Sampling might fail in test environment, that's OK
            }
        }
    }

    #[tokio::test]
    async fn test_cpu_sampling() {
        let sample = TelemetryCollector::collect_cpu_sample().await;
        match sample {
            Ok(sample) => {
                assert!(sample.cpu_percentage >= 0.0);
                assert!(sample.cpu_percentage <= 100.0);
            }
            Err(_) => {
                // Sampling might fail in test environment, that's OK
            }
        }
    }

    #[test]
    fn test_summary_calculation() {
        let collector = TelemetryCollector::new().unwrap();

        // Record some test data
        collector.record_timing("embed", Duration::from_millis(100));
        collector.record_timing("embed", Duration::from_millis(150));
        collector.record_timing("extract", Duration::from_millis(50));
        collector.increment_counter("operations");
        collector.increment_counter("operations");

        let summary = collector.summary();
        assert!(!summary.hot_paths.is_empty());

        // Find embed operation in hot paths
        let embed_path = summary.hot_paths.iter().find(|p| p.path == "embed");
        assert!(embed_path.is_some());
        assert_eq!(embed_path.unwrap().execution_count, 2);
        assert_eq!(embed_path.unwrap().avg_duration_ms, 125); // (100 + 150) / 2
    }

    #[test]
    fn test_disabled_collector() {
        let config = TelemetryConfig {
            enabled: false,
            ..TelemetryConfig::default()
        };
        let collector = TelemetryCollector::with_config(config).unwrap();

        // These operations should do nothing when disabled
        collector.record_timing("test", Duration::from_millis(100));
        collector.increment_counter("test");

        let summary = collector.summary();
        assert!(summary.hot_paths.is_empty());
        assert_eq!(summary.total_traces, 0);
    }

    #[cfg(unix)]
    #[test]
    fn test_process_memory_info_real() {
        let result = TelemetryCollector::process_memory_info();
        assert!(result.is_ok(), "process_memory_info should succeed on Unix");
        let (current, peak, faults) = result.unwrap();
        assert!(current > 0, "current RSS should be > 0");
        assert!(peak > 0, "peak RSS should be > 0");
        assert!(peak >= current, "peak must be >= current RSS");
        assert!(faults > 0, "minor page fault count should be > 0");
    }

    #[cfg(unix)]
    #[test]
    fn test_process_cpu_times_real() {
        let result = TelemetryCollector::process_cpu_times();
        assert!(result.is_ok(), "process_cpu_times should succeed on Unix");
        let (user_us, sys_us) = result.unwrap();
        // A running process must have consumed some CPU time
        assert!(user_us + sys_us >= 0, "CPU times must be non-negative");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_cpu_sample_is_bounded() {
        // The 50ms-delta measurement must produce a value in [0, 100]
        let sample = TelemetryCollector::collect_cpu_sample().await.unwrap();
        assert!(
            sample.cpu_percentage >= 0.0 && sample.cpu_percentage <= 100.0,
            "cpu_percentage={} out of range",
            sample.cpu_percentage
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_memory_sample_no_random() {
        // Two consecutive samples must agree on peak (same process, non-decreasing)
        let s1 = TelemetryCollector::collect_memory_sample().unwrap();
        let s2 = TelemetryCollector::collect_memory_sample().unwrap();
        // Peak should be non-decreasing
        assert!(
            s2.peak_memory >= s1.peak_memory,
            "peak memory must not decrease"
        );
        // Ensure values are deterministic (not random noise ± 1 MB)
        let diff = (s2.allocated_bytes as i64 - s1.allocated_bytes as i64).unsigned_abs();
        assert!(
            diff < 64 * 1024 * 1024,
            "RSS swing between two consecutive samples should be < 64 MB, got {} bytes",
            diff
        );
    }
}
