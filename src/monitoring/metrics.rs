use prometheus::{
    exponential_buckets, linear_buckets, Gauge, Histogram, HistogramOpts, IntCounter, IntGauge,
    Opts, Registry,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Prometheus metrics collector for the memory system
pub struct MetricsCollector {
    registry: Arc<Registry>,

    // Request metrics
    pub requests_total: IntCounter,
    pub requests_duration_seconds: Histogram,
    pub requests_in_flight: IntGauge,

    // Memory tier metrics
    pub memories_by_tier: IntGauge,
    pub memory_migrations_total: IntCounter,
    pub memory_creation_total: IntCounter,
    pub memory_deletion_total: IntCounter,

    // Database metrics
    pub db_connections_active: IntGauge,
    pub db_connections_max: IntGauge,
    pub db_query_duration_seconds: Histogram,
    pub db_query_errors_total: IntCounter,

    // Search metrics
    pub search_requests_total: IntCounter,
    pub search_duration_seconds: Histogram,
    pub search_results_count: Histogram,
    pub search_cache_hits_total: IntCounter,
    pub search_cache_misses_total: IntCounter,

    // System metrics
    pub memory_usage_bytes: Gauge,
    pub cpu_usage_percent: Gauge,
    pub uptime_seconds: IntCounter,
    pub error_rate_percent: Gauge,

    // Migration metrics
    pub migration_duration_seconds: Histogram,
    pub migration_failures_total: IntCounter,
    pub migration_queue_size: IntGauge,

    // Performance metrics
    pub response_time_p95: Gauge,
    pub response_time_p99: Gauge,
    pub memory_pressure_ratio: Gauge,
}

impl MetricsCollector {
    pub fn new() -> anyhow::Result<Self> {
        let registry = Arc::new(Registry::new());

        let requests_total = IntCounter::with_opts(Opts::new(
            "memory_requests_total",
            "Total number of memory requests",
        ))?;
        registry.register(Box::new(requests_total.clone()))?;

        let requests_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "memory_request_duration_seconds",
                "Duration of memory requests in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 15)?),
        )?;
        registry.register(Box::new(requests_duration_seconds.clone()))?;

        let requests_in_flight = IntGauge::with_opts(Opts::new(
            "memory_requests_in_flight",
            "Number of requests currently being processed",
        ))?;
        registry.register(Box::new(requests_in_flight.clone()))?;

        let memories_by_tier = IntGauge::with_opts(Opts::new(
            "memory_tier_count",
            "Number of memories in each tier",
        ))?;
        registry.register(Box::new(memories_by_tier.clone()))?;

        let memory_migrations_total = IntCounter::with_opts(Opts::new(
            "memory_migrations_total",
            "Total number of memory tier migrations",
        ))?;
        registry.register(Box::new(memory_migrations_total.clone()))?;

        let memory_creation_total = IntCounter::with_opts(Opts::new(
            "memory_creation_total",
            "Total number of memories created",
        ))?;
        registry.register(Box::new(memory_creation_total.clone()))?;

        let memory_deletion_total = IntCounter::with_opts(Opts::new(
            "memory_deletion_total",
            "Total number of memories deleted",
        ))?;
        registry.register(Box::new(memory_deletion_total.clone()))?;

        let db_connections_active = IntGauge::with_opts(Opts::new(
            "db_connections_active",
            "Number of active database connections",
        ))?;
        registry.register(Box::new(db_connections_active.clone()))?;

        let db_connections_max = IntGauge::with_opts(Opts::new(
            "db_connections_max",
            "Maximum number of database connections",
        ))?;
        registry.register(Box::new(db_connections_max.clone()))?;

        let db_query_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "db_query_duration_seconds",
                "Duration of database queries in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 15)?),
        )?;
        registry.register(Box::new(db_query_duration_seconds.clone()))?;

        let db_query_errors_total = IntCounter::with_opts(Opts::new(
            "db_query_errors_total",
            "Total number of database query errors",
        ))?;
        registry.register(Box::new(db_query_errors_total.clone()))?;

        let search_requests_total = IntCounter::with_opts(Opts::new(
            "search_requests_total",
            "Total number of search requests",
        ))?;
        registry.register(Box::new(search_requests_total.clone()))?;

        let search_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "search_duration_seconds",
                "Duration of search requests in seconds",
            )
            .buckets(linear_buckets(0.01, 0.01, 20)?),
        )?;
        registry.register(Box::new(search_duration_seconds.clone()))?;

        let search_results_count = Histogram::with_opts(
            HistogramOpts::new(
                "search_results_count",
                "Number of results returned by search",
            )
            .buckets(linear_buckets(1.0, 5.0, 20)?),
        )?;
        registry.register(Box::new(search_results_count.clone()))?;

        let search_cache_hits_total = IntCounter::with_opts(Opts::new(
            "search_cache_hits_total",
            "Total number of search cache hits",
        ))?;
        registry.register(Box::new(search_cache_hits_total.clone()))?;

        let search_cache_misses_total = IntCounter::with_opts(Opts::new(
            "search_cache_misses_total",
            "Total number of search cache misses",
        ))?;
        registry.register(Box::new(search_cache_misses_total.clone()))?;

        let memory_usage_bytes = Gauge::with_opts(Opts::new(
            "memory_usage_bytes",
            "Current memory usage in bytes",
        ))?;
        registry.register(Box::new(memory_usage_bytes.clone()))?;

        let cpu_usage_percent = Gauge::with_opts(Opts::new(
            "cpu_usage_percent",
            "Current CPU usage percentage",
        ))?;
        registry.register(Box::new(cpu_usage_percent.clone()))?;

        let uptime_seconds =
            IntCounter::with_opts(Opts::new("uptime_seconds_total", "Total uptime in seconds"))?;
        registry.register(Box::new(uptime_seconds.clone()))?;

        let error_rate_percent = Gauge::with_opts(Opts::new(
            "error_rate_percent",
            "Current error rate percentage",
        ))?;
        registry.register(Box::new(error_rate_percent.clone()))?;

        let migration_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "migration_duration_seconds",
                "Duration of memory migrations in seconds",
            )
            .buckets(exponential_buckets(0.01, 2.0, 12)?),
        )?;
        registry.register(Box::new(migration_duration_seconds.clone()))?;

        let migration_failures_total = IntCounter::with_opts(Opts::new(
            "migration_failures_total",
            "Total number of migration failures",
        ))?;
        registry.register(Box::new(migration_failures_total.clone()))?;

        let migration_queue_size = IntGauge::with_opts(Opts::new(
            "migration_queue_size",
            "Number of memories queued for migration",
        ))?;
        registry.register(Box::new(migration_queue_size.clone()))?;

        let response_time_p95 = Gauge::with_opts(Opts::new(
            "response_time_p95_seconds",
            "95th percentile response time in seconds",
        ))?;
        registry.register(Box::new(response_time_p95.clone()))?;

        let response_time_p99 = Gauge::with_opts(Opts::new(
            "response_time_p99_seconds",
            "99th percentile response time in seconds",
        ))?;
        registry.register(Box::new(response_time_p99.clone()))?;

        let memory_pressure_ratio = Gauge::with_opts(Opts::new(
            "memory_pressure_ratio",
            "Ratio of memory usage indicating pressure (0-1)",
        ))?;
        registry.register(Box::new(memory_pressure_ratio.clone()))?;

        info!("Initialized Prometheus metrics collector");

        Ok(Self {
            registry,
            requests_total,
            requests_duration_seconds,
            requests_in_flight,
            memories_by_tier,
            memory_migrations_total,
            memory_creation_total,
            memory_deletion_total,
            db_connections_active,
            db_connections_max,
            db_query_duration_seconds,
            db_query_errors_total,
            search_requests_total,
            search_duration_seconds,
            search_results_count,
            search_cache_hits_total,
            search_cache_misses_total,
            memory_usage_bytes,
            cpu_usage_percent,
            uptime_seconds,
            error_rate_percent,
            migration_duration_seconds,
            migration_failures_total,
            migration_queue_size,
            response_time_p95,
            response_time_p99,
            memory_pressure_ratio,
        })
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    /// Record a request with timing
    pub fn record_request(&self, start_time: Instant) {
        let duration = start_time.elapsed().as_secs_f64();
        self.requests_total.inc();
        self.requests_duration_seconds.observe(duration);
    }

    /// Record a database query with timing
    pub fn record_db_query(&self, start_time: Instant, success: bool) {
        let duration = start_time.elapsed().as_secs_f64();
        self.db_query_duration_seconds.observe(duration);

        if !success {
            self.db_query_errors_total.inc();
        }
    }

    /// Record a search operation
    pub fn record_search(&self, start_time: Instant, results_count: usize, cache_hit: bool) {
        let duration = start_time.elapsed().as_secs_f64();
        self.search_requests_total.inc();
        self.search_duration_seconds.observe(duration);
        self.search_results_count.observe(results_count as f64);

        if cache_hit {
            self.search_cache_hits_total.inc();
        } else {
            self.search_cache_misses_total.inc();
        }
    }

    /// Record a memory migration
    pub fn record_migration(
        &self,
        start_time: Instant,
        success: bool,
        _memory_id: Uuid,
        from_tier: &str,
        to_tier: &str,
    ) {
        let duration = start_time.elapsed().as_secs_f64();
        self.migration_duration_seconds.observe(duration);

        if success {
            self.memory_migrations_total.inc();
            info!(
                "Recorded successful migration from {} to {} in {:.3}s",
                from_tier, to_tier, duration
            );
        } else {
            self.migration_failures_total.inc();
            warn!(
                "Recorded failed migration from {} to {} after {:.3}s",
                from_tier, to_tier, duration
            );
        }
    }

    /// Update system resource metrics
    pub fn update_system_metrics(&self, memory_bytes: u64, cpu_percent: f64) {
        self.memory_usage_bytes.set(memory_bytes as f64);
        self.cpu_usage_percent.set(cpu_percent);
    }

    /// Update database connection pool metrics
    pub fn update_connection_pool_metrics(&self, active: u32, max: u32) {
        self.db_connections_active.set(active as i64);
        self.db_connections_max.set(max as i64);
    }

    /// Update memory tier distribution
    pub fn update_tier_metrics(&self, working: u64, warm: u64, cold: u64) {
        // Use labels to distinguish tiers (simplified for now)
        // In production, would use proper label support
        info!(
            "Memory tier distribution - Working: {}, Warm: {}, Cold: {}",
            working, warm, cold
        );
    }

    /// Calculate and update derived metrics
    pub fn update_derived_metrics(&self) {
        // Calculate cache hit ratio
        let cache_hits = self.search_cache_hits_total.get();
        let cache_misses = self.search_cache_misses_total.get();
        let total_requests = cache_hits + cache_misses;

        if total_requests > 0 {
            let hit_ratio = cache_hits as f64 / total_requests as f64;
            info!("Search cache hit ratio: {:.2}%", hit_ratio * 100.0);
        }

        // Calculate error rate
        let total_requests = self.requests_total.get();
        let db_errors = self.db_query_errors_total.get();
        let migration_failures = self.migration_failures_total.get();

        if total_requests > 0 {
            let error_rate =
                (db_errors + migration_failures) as f64 / total_requests as f64 * 100.0;
            self.error_rate_percent.set(error_rate);
        }
    }

    /// Get metrics in Prometheus format
    pub fn gather_metrics(&self) -> String {
        use prometheus::TextEncoder;
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder
            .encode_to_string(&metric_families)
            .unwrap_or_else(|e| {
                error!("Failed to encode metrics: {}", e);
                String::new()
            })
    }
}

/// Request timing guard that automatically records metrics on drop
pub struct RequestTimer {
    start: Instant,
    metrics: Arc<MetricsCollector>,
    #[allow(dead_code)]
    operation: String,
}

impl RequestTimer {
    pub fn new(metrics: Arc<MetricsCollector>, operation: String) -> Self {
        metrics.requests_in_flight.inc();
        Self {
            start: Instant::now(),
            metrics,
            operation,
        }
    }
}

impl Drop for RequestTimer {
    fn drop(&mut self) {
        self.metrics.requests_in_flight.dec();
        self.metrics.record_request(self.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new().unwrap();
        assert_eq!(collector.requests_total.get(), 0);
        assert_eq!(collector.requests_in_flight.get(), 0);
    }

    #[test]
    fn test_request_timing() {
        let collector = MetricsCollector::new().unwrap();
        let start = Instant::now();

        // Simulate some work
        thread::sleep(Duration::from_millis(10));

        collector.record_request(start);
        assert_eq!(collector.requests_total.get(), 1);

        let metrics_text = collector.gather_metrics();
        assert!(metrics_text.contains("memory_requests_total"));
    }

    #[test]
    fn test_request_timer() {
        let collector = Arc::new(MetricsCollector::new().unwrap());

        {
            let _timer = RequestTimer::new(collector.clone(), "test".to_string());
            assert_eq!(collector.requests_in_flight.get(), 1);
            thread::sleep(Duration::from_millis(5));
        } // Timer drops here

        assert_eq!(collector.requests_in_flight.get(), 0);
        assert_eq!(collector.requests_total.get(), 1);
    }

    #[test]
    fn test_system_metrics_update() {
        let collector = MetricsCollector::new().unwrap();

        collector.update_system_metrics(1024 * 1024 * 512, 75.5); // 512MB, 75.5% CPU
        assert_eq!(collector.memory_usage_bytes.get(), 1024.0 * 1024.0 * 512.0);
        assert_eq!(collector.cpu_usage_percent.get(), 75.5);
    }

    #[test]
    fn test_db_metrics() {
        let collector = MetricsCollector::new().unwrap();
        let start = Instant::now();

        collector.record_db_query(start, true);
        assert_eq!(collector.db_query_errors_total.get(), 0);

        collector.record_db_query(start, false);
        assert_eq!(collector.db_query_errors_total.get(), 1);
    }
}
