//! Performance metrics collection and analysis

use anyhow::Result;
use prometheus::{Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Registry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

/// Performance metrics collector
pub struct MetricsCollector {
    registry: Registry,

    // Latency metrics
    operation_latency: HashMap<String, Histogram>,

    // Throughput metrics
    requests_total: IntCounter,
    requests_success: IntCounter,
    requests_failed: IntCounter,

    // System metrics
    cpu_usage: Gauge,
    memory_usage: Gauge,
    db_connections: IntGauge,

    // Cache metrics
    cache_hits: IntCounter,
    cache_misses: IntCounter,

    // Custom metrics
    custom_metrics: Arc<RwLock<HashMap<String, f64>>>,
}

impl MetricsCollector {
    pub fn new() -> Result<Self> {
        let registry = Registry::new();

        // Create latency histograms
        let mut operation_latency = HashMap::new();

        for operation in &["create", "read", "update", "delete", "search", "migrate"] {
            let histogram = Histogram::with_opts(
                HistogramOpts::new(
                    format!("{operation}_latency"),
                    format!("Latency for {operation} operations"),
                )
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            )?;

            registry.register(Box::new(histogram.clone()))?;
            operation_latency.insert(operation.to_string(), histogram);
        }

        // Create counters
        let requests_total = IntCounter::new("requests_total", "Total number of requests")?;
        let requests_success = IntCounter::new("requests_success", "Total successful requests")?;
        let requests_failed = IntCounter::new("requests_failed", "Total failed requests")?;
        let cache_hits = IntCounter::new("cache_hits", "Total cache hits")?;
        let cache_misses = IntCounter::new("cache_misses", "Total cache misses")?;

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(requests_success.clone()))?;
        registry.register(Box::new(requests_failed.clone()))?;
        registry.register(Box::new(cache_hits.clone()))?;
        registry.register(Box::new(cache_misses.clone()))?;

        // Create gauges
        let cpu_usage = Gauge::new("cpu_usage_percent", "Current CPU usage percentage")?;
        let memory_usage = Gauge::new("memory_usage_bytes", "Current memory usage in bytes")?;
        let db_connections = IntGauge::new("db_connections_active", "Active database connections")?;

        registry.register(Box::new(cpu_usage.clone()))?;
        registry.register(Box::new(memory_usage.clone()))?;
        registry.register(Box::new(db_connections.clone()))?;

        Ok(Self {
            registry,
            operation_latency,
            requests_total,
            requests_success,
            requests_failed,
            cpu_usage,
            memory_usage,
            db_connections,
            cache_hits,
            cache_misses,
            custom_metrics: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Record operation latency
    pub fn record_latency(&self, operation: &str, duration: Duration) {
        if let Some(histogram) = self.operation_latency.get(operation) {
            histogram.observe(duration.as_secs_f64());
        }
    }

    /// Record successful request
    pub fn record_success(&self) {
        self.requests_total.inc();
        self.requests_success.inc();
    }

    /// Record failed request
    pub fn record_failure(&self) {
        self.requests_total.inc();
        self.requests_failed.inc();
    }

    /// Record cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.inc();
    }

    /// Record cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.inc();
    }

    /// Update CPU usage
    pub fn update_cpu_usage(&self, usage: f64) {
        self.cpu_usage.set(usage);
    }

    /// Update memory usage
    pub fn update_memory_usage(&self, bytes: f64) {
        self.memory_usage.set(bytes);
    }

    /// Update database connections
    pub fn update_db_connections(&self, connections: i64) {
        self.db_connections.set(connections);
    }

    /// Record custom metric
    pub async fn record_custom_metric(&self, name: &str, value: f64) {
        let mut metrics = self.custom_metrics.write().await;
        metrics.insert(name.to_string(), value);
    }

    /// Get current metrics snapshot
    pub async fn get_snapshot(&self) -> MetricsSnapshot {
        let custom_metrics = self.custom_metrics.read().await;

        // Calculate cache hit ratio
        let cache_hit_ratio = {
            let hits = self.cache_hits.get() as f64;
            let misses = self.cache_misses.get() as f64;
            let total = hits + misses;

            if total > 0.0 {
                (hits / total) * 100.0
            } else {
                0.0
            }
        };

        // Calculate success rate
        let success_rate = {
            let success = self.requests_success.get() as f64;
            let total = self.requests_total.get() as f64;

            if total > 0.0 {
                (success / total) * 100.0
            } else {
                100.0
            }
        };

        MetricsSnapshot {
            timestamp: Utc::now(),
            requests_total: self.requests_total.get(),
            requests_success: self.requests_success.get(),
            requests_failed: self.requests_failed.get(),
            success_rate,
            cpu_usage: self.cpu_usage.get(),
            memory_usage: self.memory_usage.get() as u64,
            db_connections: self.db_connections.get(),
            cache_hits: self.cache_hits.get(),
            cache_misses: self.cache_misses.get(),
            cache_hit_ratio,
            custom_metrics: custom_metrics.clone(),
        }
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    /// Reset all metrics
    pub async fn reset(&self) {
        self.requests_total.reset();
        self.requests_success.reset();
        self.requests_failed.reset();
        self.cache_hits.reset();
        self.cache_misses.reset();
        self.cpu_usage.set(0.0);
        self.memory_usage.set(0.0);
        self.db_connections.set(0);

        let mut custom = self.custom_metrics.write().await;
        custom.clear();
    }
}

/// Snapshot of current metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_failed: u64,
    pub success_rate: f64,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub db_connections: i64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_ratio: f64,
    pub custom_metrics: HashMap<String, f64>,
}

/// Operation timer for automatic latency recording
pub struct OperationTimer<'a> {
    collector: &'a MetricsCollector,
    operation: String,
    start: Instant,
}

impl<'a> OperationTimer<'a> {
    pub fn new(collector: &'a MetricsCollector, operation: &str) -> Self {
        Self {
            collector,
            operation: operation.to_string(),
            start: Instant::now(),
        }
    }
}

impl<'a> Drop for OperationTimer<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.collector.record_latency(&self.operation, duration);
        debug!("Operation '{}' took {:?}", self.operation, duration);
    }
}

/// Performance analyzer for metrics analysis
pub struct PerformanceAnalyzer;

impl PerformanceAnalyzer {
    /// Analyze metrics and identify issues
    pub fn analyze(snapshot: &MetricsSnapshot) -> PerformanceAnalysis {
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        // Check success rate
        if snapshot.success_rate < 99.0 {
            issues.push(format!(
                "Low success rate: {:.2}% (target: >99%)",
                snapshot.success_rate
            ));
            recommendations.push("Investigate error logs and increase error handling".to_string());
        }

        // Check cache hit ratio
        if snapshot.cache_hit_ratio < 80.0 && snapshot.cache_hits + snapshot.cache_misses > 100 {
            issues.push(format!(
                "Low cache hit ratio: {:.2}% (target: >80%)",
                snapshot.cache_hit_ratio
            ));
            recommendations.push("Review cache configuration and increase cache size".to_string());
        }

        // Check CPU usage
        if snapshot.cpu_usage > 80.0 {
            issues.push(format!(
                "High CPU usage: {:.2}% (threshold: 80%)",
                snapshot.cpu_usage
            ));
            recommendations.push("Profile CPU usage and optimize hot paths".to_string());
        }

        // Check memory usage
        let memory_gb = snapshot.memory_usage as f64 / (1024.0 * 1024.0 * 1024.0);
        if memory_gb > 4.0 {
            issues.push(format!(
                "High memory usage: {memory_gb:.2} GB (threshold: 4 GB)"
            ));
            recommendations
                .push("Investigate memory leaks and optimize data structures".to_string());
        }

        // Check database connections
        if snapshot.db_connections > 80 {
            issues.push(format!(
                "High database connection count: {} (threshold: 80)",
                snapshot.db_connections
            ));
            recommendations.push("Review connection pooling and query optimization".to_string());
        }

        let health = if issues.is_empty() {
            PerformanceHealth::Good
        } else if issues.len() <= 2 {
            PerformanceHealth::Warning
        } else {
            PerformanceHealth::Critical
        };

        PerformanceAnalysis {
            timestamp: snapshot.timestamp,
            health,
            issues,
            recommendations,
            metrics_summary: format!(
                "Requests: {} | Success Rate: {:.2}% | Cache Hit: {:.2}% | CPU: {:.2}% | Memory: {:.2} GB",
                snapshot.requests_total,
                snapshot.success_rate,
                snapshot.cache_hit_ratio,
                snapshot.cpu_usage,
                memory_gb
            ),
        }
    }
}

/// Performance analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub health: PerformanceHealth,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
    pub metrics_summary: String,
}

/// Performance health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceHealth {
    Good,
    Warning,
    Critical,
}

use chrono::Utc;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new().unwrap();

        // Record some metrics
        collector.record_success();
        collector.record_success();
        collector.record_failure();
        collector.record_cache_hit();
        collector.record_cache_miss();
        collector.update_cpu_usage(50.0);
        collector.update_memory_usage(1_000_000_000.0);

        // Get snapshot
        let snapshot = collector.get_snapshot().await;

        assert_eq!(snapshot.requests_total, 3);
        assert_eq!(snapshot.requests_success, 2);
        assert_eq!(snapshot.requests_failed, 1);
        assert_eq!(snapshot.cache_hits, 1);
        assert_eq!(snapshot.cache_misses, 1);
        assert_eq!(snapshot.cache_hit_ratio, 50.0);
        assert_eq!(snapshot.cpu_usage, 50.0);
    }

    #[tokio::test]
    async fn test_operation_timer() {
        let collector = MetricsCollector::new().unwrap();

        {
            let _timer = OperationTimer::new(&collector, "read");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Timer should have recorded latency when dropped
        // We can't easily test the exact value, but we can verify it doesn't panic
    }

    #[tokio::test]
    async fn test_performance_analyzer() {
        let mut snapshot = MetricsSnapshot {
            timestamp: Utc::now(),
            requests_total: 1000,
            requests_success: 950,
            requests_failed: 50,
            success_rate: 95.0,
            cpu_usage: 85.0,
            memory_usage: 5_000_000_000,
            db_connections: 90,
            cache_hits: 200,
            cache_misses: 100,
            cache_hit_ratio: 66.67,
            custom_metrics: HashMap::new(),
        };

        let analysis = PerformanceAnalyzer::analyze(&snapshot);

        assert!(matches!(analysis.health, PerformanceHealth::Critical));
        assert!(!analysis.issues.is_empty());
        assert!(!analysis.recommendations.is_empty());

        // Test with good metrics
        snapshot.success_rate = 99.5;
        snapshot.cpu_usage = 50.0;
        snapshot.memory_usage = 1_000_000_000;
        snapshot.db_connections = 20;
        snapshot.cache_hit_ratio = 90.0;

        let analysis = PerformanceAnalyzer::analyze(&snapshot);
        assert!(matches!(analysis.health, PerformanceHealth::Good));
        assert!(analysis.issues.is_empty());
    }
}
