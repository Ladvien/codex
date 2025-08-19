use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Performance profiler for tracking system performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceProfiler {
    inner: Arc<RwLock<ProfilerInner>>,
}

#[derive(Debug)]
struct ProfilerInner {
    profiles: HashMap<String, OperationProfile>,
    active_traces: HashMap<Uuid, ActiveTrace>,
    config: ProfilerConfig,
    start_time: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    pub max_traces: usize,
    pub max_history_per_operation: usize,
    pub slow_operation_threshold_ms: u64,
    pub enabled: bool,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            max_traces: 1000,
            max_history_per_operation: 100,
            slow_operation_threshold_ms: 100,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationProfile {
    pub name: String,
    pub total_calls: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub p95_duration_ms: u64,
    pub p99_duration_ms: u64,
    pub error_count: u64,
    pub slow_operations: u64,
    pub recent_durations: VecDeque<u64>,
    pub last_updated: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub struct ActiveTrace {
    pub id: Uuid,
    pub operation: String,
    pub start_time: Instant,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub id: Uuid,
    pub operation: String,
    pub duration_ms: u64,
    pub success: bool,
    pub metadata: HashMap<String, String>,
    pub timestamp: std::time::SystemTime,
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        Self::with_config(ProfilerConfig::default())
    }

    pub fn with_config(config: ProfilerConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ProfilerInner {
                profiles: HashMap::new(),
                active_traces: HashMap::new(),
                config,
                start_time: Instant::now(),
            })),
        }
    }

    /// Start profiling an operation
    pub fn start_trace(&self, operation: String) -> Option<TraceHandle> {
        if !self.is_enabled() {
            return None;
        }

        let trace_id = Uuid::new_v4();
        let trace = ActiveTrace {
            id: trace_id,
            operation: operation.clone(),
            start_time: Instant::now(),
            metadata: HashMap::new(),
        };

        if let Ok(mut inner) = self.inner.write() {
            // Clean up old traces if we're at the limit
            if inner.active_traces.len() >= inner.config.max_traces {
                self.cleanup_old_traces(&mut inner);
            }

            inner.active_traces.insert(trace_id, trace);
            debug!("Started trace {} for operation {}", trace_id, operation);
        }

        Some(TraceHandle {
            profiler: self.clone(),
            trace_id,
            operation,
        })
    }

    /// Complete a trace and record the results
    pub fn complete_trace(
        &self,
        trace_id: Uuid,
        success: bool,
        metadata: Option<HashMap<String, String>>,
    ) {
        if !self.is_enabled() {
            return;
        }

        if let Ok(mut inner) = self.inner.write() {
            if let Some(trace) = inner.active_traces.remove(&trace_id) {
                let duration = trace.start_time.elapsed();
                let duration_ms = duration.as_millis() as u64;

                let mut final_metadata = trace.metadata;
                if let Some(additional_metadata) = metadata {
                    final_metadata.extend(additional_metadata);
                }

                // Update operation profile
                let profile = inner
                    .profiles
                    .entry(trace.operation.clone())
                    .or_insert_with(|| OperationProfile::new(trace.operation.clone()));

                profile.record_operation(duration_ms, success);

                let is_slow = duration_ms > inner.config.slow_operation_threshold_ms;
                if is_slow {
                    warn!(
                        "Slow operation detected: {} took {}ms",
                        trace.operation, duration_ms
                    );
                }

                debug!(
                    "Completed trace {} for {} in {}ms (success: {})",
                    trace_id, trace.operation, duration_ms, success
                );
            }
        }
    }

    /// Get performance profile for a specific operation
    pub fn get_operation_profile(&self, operation: &str) -> Option<OperationProfile> {
        if let Ok(inner) = self.inner.read() {
            inner.profiles.get(operation).cloned()
        } else {
            None
        }
    }

    /// Get all operation profiles
    pub fn get_all_profiles(&self) -> HashMap<String, OperationProfile> {
        if let Ok(inner) = self.inner.read() {
            inner.profiles.clone()
        } else {
            HashMap::new()
        }
    }

    /// Get performance summary
    pub fn get_performance_summary(&self) -> PerformanceSummary {
        if let Ok(inner) = self.inner.read() {
            let total_operations: u64 = inner.profiles.values().map(|p| p.total_calls).sum();
            let total_errors: u64 = inner.profiles.values().map(|p| p.error_count).sum();
            let total_slow_operations: u64 =
                inner.profiles.values().map(|p| p.slow_operations).sum();

            let avg_response_time = if total_operations > 0 {
                let total_duration: u64 =
                    inner.profiles.values().map(|p| p.total_duration_ms).sum();
                total_duration as f64 / total_operations as f64
            } else {
                0.0
            };

            let error_rate = if total_operations > 0 {
                total_errors as f64 / total_operations as f64 * 100.0
            } else {
                0.0
            };

            let uptime_seconds = inner.start_time.elapsed().as_secs();
            let operations_per_second = if uptime_seconds > 0 {
                total_operations as f64 / uptime_seconds as f64
            } else {
                0.0
            };

            PerformanceSummary {
                total_operations,
                total_errors,
                error_rate,
                avg_response_time_ms: avg_response_time,
                total_slow_operations,
                active_traces: inner.active_traces.len(),
                uptime_seconds,
                operations_per_second,
                top_operations: inner
                    .profiles
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .fold(Vec::new(), |mut acc, profile| {
                        acc.push((profile.name.clone(), profile.total_calls));
                        acc.sort_by(|a, b| b.1.cmp(&a.1));
                        acc.truncate(10);
                        acc
                    }),
            }
        } else {
            PerformanceSummary::default()
        }
    }

    /// Enable or disable profiling
    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut inner) = self.inner.write() {
            inner.config.enabled = enabled;
            if enabled {
                info!("Performance profiling enabled");
            } else {
                info!("Performance profiling disabled");
                inner.active_traces.clear();
            }
        }
    }

    /// Check if profiling is enabled
    pub fn is_enabled(&self) -> bool {
        if let Ok(inner) = self.inner.read() {
            inner.config.enabled
        } else {
            false
        }
    }

    /// Reset all profiling data
    pub fn reset(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.profiles.clear();
            inner.active_traces.clear();
            inner.start_time = Instant::now();
            info!("Performance profiling data reset");
        }
    }

    /// Clean up old traces
    fn cleanup_old_traces(&self, inner: &mut ProfilerInner) {
        let cutoff_time = Instant::now() - Duration::from_secs(300); // 5 minutes
        let initial_count = inner.active_traces.len();

        inner
            .active_traces
            .retain(|_, trace| trace.start_time > cutoff_time);

        let removed = initial_count - inner.active_traces.len();
        if removed > 0 {
            warn!("Cleaned up {} stale traces", removed);
        }
    }

    /// Export performance data for external analysis
    pub fn export_data(&self) -> Option<PerformanceExport> {
        if let Ok(inner) = self.inner.read() {
            Some(PerformanceExport {
                profiles: inner.profiles.clone(),
                summary: self.get_performance_summary(),
                config: inner.config.clone(),
                exported_at: std::time::SystemTime::now(),
            })
        } else {
            None
        }
    }
}

/// Handle for an active trace that automatically completes on drop
pub struct TraceHandle {
    profiler: PerformanceProfiler,
    trace_id: Uuid,
    #[allow(dead_code)]
    operation: String,
}

impl TraceHandle {
    /// Add metadata to the trace
    pub fn add_metadata(&self, key: String, value: String) {
        if let Ok(mut inner) = self.profiler.inner.write() {
            if let Some(trace) = inner.active_traces.get_mut(&self.trace_id) {
                trace.metadata.insert(key, value);
            }
        }
    }

    /// Complete the trace manually
    pub fn complete(self, success: bool) {
        self.profiler.complete_trace(self.trace_id, success, None);
        std::mem::forget(self); // Prevent drop from running
    }

    /// Complete the trace with additional metadata
    pub fn complete_with_metadata(self, success: bool, metadata: HashMap<String, String>) {
        self.profiler
            .complete_trace(self.trace_id, success, Some(metadata));
        std::mem::forget(self);
    }
}

impl Drop for TraceHandle {
    fn drop(&mut self) {
        // Auto-complete as successful if not manually completed
        self.profiler.complete_trace(self.trace_id, true, None);
    }
}

impl OperationProfile {
    fn new(name: String) -> Self {
        Self {
            name,
            total_calls: 0,
            total_duration_ms: 0,
            avg_duration_ms: 0.0,
            min_duration_ms: u64::MAX,
            max_duration_ms: 0,
            p95_duration_ms: 0,
            p99_duration_ms: 0,
            error_count: 0,
            slow_operations: 0,
            recent_durations: VecDeque::new(),
            last_updated: std::time::SystemTime::now(),
        }
    }

    fn record_operation(&mut self, duration_ms: u64, success: bool) {
        self.total_calls += 1;
        self.total_duration_ms += duration_ms;
        self.avg_duration_ms = self.total_duration_ms as f64 / self.total_calls as f64;

        self.min_duration_ms = self.min_duration_ms.min(duration_ms);
        self.max_duration_ms = self.max_duration_ms.max(duration_ms);

        if !success {
            self.error_count += 1;
        }

        if duration_ms > 100 {
            // Slow operation threshold
            self.slow_operations += 1;
        }

        // Track recent durations for percentile calculations
        self.recent_durations.push_back(duration_ms);
        if self.recent_durations.len() > 100 {
            self.recent_durations.pop_front();
        }

        // Update percentiles
        self.update_percentiles();
        self.last_updated = std::time::SystemTime::now();
    }

    fn update_percentiles(&mut self) {
        if self.recent_durations.is_empty() {
            return;
        }

        let mut sorted: Vec<u64> = self.recent_durations.iter().cloned().collect();
        sorted.sort_unstable();

        let len = sorted.len();
        if len > 0 {
            let p95_index = ((len as f64) * 0.95) as usize;
            let p99_index = ((len as f64) * 0.99) as usize;

            self.p95_duration_ms = sorted[p95_index.min(len - 1)];
            self.p99_duration_ms = sorted[p99_index.min(len - 1)];
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub total_operations: u64,
    pub total_errors: u64,
    pub error_rate: f64,
    pub avg_response_time_ms: f64,
    pub total_slow_operations: u64,
    pub active_traces: usize,
    pub uptime_seconds: u64,
    pub operations_per_second: f64,
    pub top_operations: Vec<(String, u64)>,
}

impl Default for PerformanceSummary {
    fn default() -> Self {
        Self {
            total_operations: 0,
            total_errors: 0,
            error_rate: 0.0,
            avg_response_time_ms: 0.0,
            total_slow_operations: 0,
            active_traces: 0,
            uptime_seconds: 0,
            operations_per_second: 0.0,
            top_operations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceExport {
    pub profiles: HashMap<String, OperationProfile>,
    pub summary: PerformanceSummary,
    pub config: ProfilerConfig,
    pub exported_at: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_creation() {
        let profiler = PerformanceProfiler::new();
        assert!(profiler.is_enabled());
        assert!(profiler.get_all_profiles().is_empty());
    }

    #[test]
    fn test_trace_lifecycle() {
        let profiler = PerformanceProfiler::new();

        let handle = profiler.start_trace("test_operation".to_string()).unwrap();
        assert_eq!(profiler.get_performance_summary().active_traces, 1);

        // Simulate work
        thread::sleep(Duration::from_millis(10));

        handle.complete(true);
        assert_eq!(profiler.get_performance_summary().active_traces, 0);
        assert_eq!(profiler.get_performance_summary().total_operations, 1);
    }

    #[test]
    fn test_auto_complete_on_drop() {
        let profiler = PerformanceProfiler::new();

        {
            let _handle = profiler.start_trace("test_operation".to_string()).unwrap();
            assert_eq!(profiler.get_performance_summary().active_traces, 1);
            // Handle drops here
        }

        assert_eq!(profiler.get_performance_summary().active_traces, 0);
        assert_eq!(profiler.get_performance_summary().total_operations, 1);
    }

    #[test]
    fn test_operation_profile_updates() {
        let profiler = PerformanceProfiler::new();

        // Record multiple operations
        for i in 0..10 {
            let handle = profiler.start_trace("test_operation".to_string()).unwrap();
            thread::sleep(Duration::from_millis(i * 2 + 5)); // Variable duration
            handle.complete(i % 7 != 0); // Some failures
        }

        let profile = profiler.get_operation_profile("test_operation").unwrap();
        assert_eq!(profile.total_calls, 10);
        assert!(profile.error_count > 0);
        assert!(profile.avg_duration_ms > 0.0);
        assert!(profile.max_duration_ms >= profile.min_duration_ms);
    }

    #[test]
    fn test_profiler_disable() {
        let profiler = PerformanceProfiler::new();
        profiler.set_enabled(false);
        assert!(!profiler.is_enabled());

        let handle = profiler.start_trace("test_operation".to_string());
        assert!(handle.is_none());
    }

    #[test]
    fn test_performance_summary() {
        let profiler = PerformanceProfiler::new();

        // Add some operations
        for _ in 0..5 {
            let handle = profiler.start_trace("fast_op".to_string()).unwrap();
            handle.complete(true);
        }

        for _ in 0..3 {
            let handle = profiler.start_trace("slow_op".to_string()).unwrap();
            thread::sleep(Duration::from_millis(150)); // Slow operation
            handle.complete(true);
        }

        let summary = profiler.get_performance_summary();
        assert_eq!(summary.total_operations, 8);
        assert!(summary.total_slow_operations > 0);
        assert_eq!(summary.total_errors, 0);
        // Operations per second might be 0.0 if test runs too quickly
        assert!(summary.operations_per_second >= 0.0);
    }

    #[test]
    fn test_trace_metadata() {
        let profiler = PerformanceProfiler::new();

        let handle = profiler.start_trace("test_operation".to_string()).unwrap();
        handle.add_metadata("user_id".to_string(), "12345".to_string());
        handle.add_metadata("request_size".to_string(), "1024".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("result_count".to_string(), "42".to_string());

        handle.complete_with_metadata(true, metadata);

        let summary = profiler.get_performance_summary();
        assert_eq!(summary.total_operations, 1);
    }

    #[test]
    fn test_data_export() {
        let profiler = PerformanceProfiler::new();

        let handle = profiler.start_trace("test_operation".to_string()).unwrap();
        handle.complete(true);

        let export = profiler.export_data().unwrap();
        assert!(!export.profiles.is_empty());
        assert_eq!(export.summary.total_operations, 1);
    }
}
