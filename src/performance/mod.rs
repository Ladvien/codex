//! Performance testing and optimization module

pub mod benchmarks;
pub mod capacity_planning;
pub mod load_testing;
pub mod metrics;
pub mod optimization;
pub mod stress_testing;

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Performance testing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceConfig {
    /// Enable performance testing
    pub enabled: bool,

    /// Load testing configuration
    pub load_testing: LoadTestConfig,

    /// Stress testing configuration
    pub stress_testing: StressTestConfig,

    /// Performance SLA thresholds
    pub sla_thresholds: SlaThresholds,

    /// Profiling configuration
    pub profiling: ProfilingConfig,
}

/// Load testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestConfig {
    /// Number of concurrent users to simulate
    pub concurrent_users: usize,

    /// Duration of load test
    pub test_duration: Duration,

    /// Ramp-up time to reach target load
    pub ramp_up_time: Duration,

    /// Target requests per second
    pub target_rps: u32,
}

/// Stress testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressTestConfig {
    /// Maximum concurrent connections to test
    pub max_connections: usize,

    /// Memory pressure threshold (percentage)
    pub memory_pressure_threshold: u8,

    /// CPU pressure threshold (percentage)
    pub cpu_pressure_threshold: u8,

    /// Enable chaos testing
    pub chaos_testing_enabled: bool,
}

/// SLA performance thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaThresholds {
    /// P50 latency threshold (ms)
    pub p50_latency_ms: u64,

    /// P95 latency threshold (ms)
    pub p95_latency_ms: u64,

    /// P99 latency threshold (ms)
    pub p99_latency_ms: u64,

    /// Minimum acceptable throughput (requests/sec)
    pub min_throughput_rps: u32,

    /// Maximum error rate (percentage)
    pub max_error_rate: f64,

    /// Cache hit ratio target (percentage)
    pub cache_hit_ratio_target: f64,
}

/// Profiling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingConfig {
    /// Enable CPU profiling
    pub cpu_profiling: bool,

    /// Enable memory profiling
    pub memory_profiling: bool,

    /// Enable I/O profiling
    pub io_profiling: bool,

    /// Sampling rate for profiling (Hz)
    pub sampling_rate_hz: u32,

    /// Profile output directory
    pub output_directory: String,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            concurrent_users: 100,
            test_duration: Duration::from_secs(300), // 5 minutes
            ramp_up_time: Duration::from_secs(60),   // 1 minute
            target_rps: 1000,
        }
    }
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            memory_pressure_threshold: 80,
            cpu_pressure_threshold: 90,
            chaos_testing_enabled: false,
        }
    }
}

impl Default for SlaThresholds {
    fn default() -> Self {
        Self {
            p50_latency_ms: 10,
            p95_latency_ms: 100,
            p99_latency_ms: 500,
            min_throughput_rps: 100,
            max_error_rate: 1.0,
            cache_hit_ratio_target: 90.0,
        }
    }
}

impl Default for ProfilingConfig {
    fn default() -> Self {
        Self {
            cpu_profiling: true,
            memory_profiling: true,
            io_profiling: true,
            sampling_rate_hz: 100,
            output_directory: "./profiles".to_string(),
        }
    }
}

/// Performance test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTestResult {
    pub test_name: String,
    pub test_type: TestType,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub duration: Duration,
    pub metrics: PerformanceMetrics,
    pub sla_violations: Vec<SlaViolation>,
    pub passed: bool,
}

/// Type of performance test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    Load,
    Stress,
    Spike,
    Soak,
    Capacity,
}

/// Performance metrics collected during testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub throughput_rps: f64,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub latency_p99_ms: u64,
    pub latency_max_ms: u64,
    pub error_rate: f64,
    pub cpu_usage_avg: f64,
    pub memory_usage_avg: f64,
    pub cache_hit_ratio: f64,
    pub db_connections_used: u32,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
}

/// SLA violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaViolation {
    pub metric: String,
    pub threshold: f64,
    pub actual_value: f64,
    pub severity: ViolationSeverity,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Severity of SLA violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}
