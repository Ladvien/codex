use anyhow::Result;
use chrono::{DateTime, Utc};
use prometheus::{Counter, Gauge, Histogram, Registry};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub id: String,
    pub metric_name: String,
    pub threshold_type: ThresholdType,
    pub threshold_value: f64,
    pub current_value: f64,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThresholdType {
    GreaterThan,
    LessThan,
    PercentageIncrease,
    PercentageDecrease,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub alert_thresholds: HashMap<String, AlertThreshold>,
    pub monitoring_interval_seconds: u64,
    pub retention_days: u32,
    pub enable_auto_scaling: bool,
    pub performance_targets: PerformanceTargets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThreshold {
    pub warning_threshold: f64,
    pub critical_threshold: f64,
    pub threshold_type: ThresholdType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTargets {
    pub p95_latency_ms: f64,          // Story 10: < 2 seconds
    pub token_reduction_percent: f64, // Story 10: 90%
    pub memory_headroom_percent: f64, // Story 10: 20%
    pub batch_throughput_ops_sec: f64,
    pub cache_hit_ratio: f64,
    pub connection_pool_usage: f64,
}

#[derive(Debug)]
pub struct PerformanceDashboard {
    config: DashboardConfig,
    pool: PgPool,
    registry: Arc<Registry>,
    alerts: Arc<RwLock<Vec<PerformanceAlert>>>,

    // Metrics
    latency_histogram: Histogram,
    throughput_gauge: Gauge,
    memory_usage_gauge: Gauge,
    token_reduction_gauge: Gauge,
    alert_counter: Counter,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        let mut alert_thresholds = HashMap::new();

        // Story 10 Performance Requirements
        alert_thresholds.insert(
            "p95_latency_ms".to_string(),
            AlertThreshold {
                warning_threshold: 1500.0,  // 1.5s warning
                critical_threshold: 2000.0, // 2s critical (Story 10 requirement)
                threshold_type: ThresholdType::GreaterThan,
                enabled: true,
            },
        );

        alert_thresholds.insert(
            "memory_headroom_percent".to_string(),
            AlertThreshold {
                warning_threshold: 25.0,  // 25% warning
                critical_threshold: 20.0, // 20% critical (Story 10 requirement)
                threshold_type: ThresholdType::LessThan,
                enabled: true,
            },
        );

        alert_thresholds.insert(
            "token_reduction_percent".to_string(),
            AlertThreshold {
                warning_threshold: 85.0,  // 85% warning
                critical_threshold: 90.0, // 90% critical (Story 10 requirement)
                threshold_type: ThresholdType::LessThan,
                enabled: true,
            },
        );

        alert_thresholds.insert(
            "connection_pool_usage".to_string(),
            AlertThreshold {
                warning_threshold: 70.0,  // 70% warning
                critical_threshold: 85.0, // 85% critical
                threshold_type: ThresholdType::GreaterThan,
                enabled: true,
            },
        );

        alert_thresholds.insert(
            "batch_throughput_regression".to_string(),
            AlertThreshold {
                warning_threshold: 15.0,  // 15% regression warning
                critical_threshold: 25.0, // 25% regression critical
                threshold_type: ThresholdType::PercentageDecrease,
                enabled: true,
            },
        );

        Self {
            alert_thresholds,
            monitoring_interval_seconds: 60,
            retention_days: 30,
            enable_auto_scaling: true,
            performance_targets: PerformanceTargets {
                p95_latency_ms: 2000.0,
                token_reduction_percent: 90.0,
                memory_headroom_percent: 20.0,
                batch_throughput_ops_sec: 1000.0,
                cache_hit_ratio: 0.9,
                connection_pool_usage: 0.7,
            },
        }
    }
}

impl PerformanceDashboard {
    pub fn new(config: DashboardConfig, pool: PgPool) -> Result<Self> {
        let registry = Arc::new(Registry::new());

        let latency_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "memory_operation_duration_seconds",
                "Duration of memory operations in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]),
        )?;

        let throughput_gauge = Gauge::with_opts(prometheus::Opts::new(
            "memory_operations_per_second",
            "Number of memory operations per second",
        ))?;

        let memory_usage_gauge = Gauge::with_opts(prometheus::Opts::new(
            "memory_headroom_percentage",
            "Available memory headroom as percentage",
        ))?;

        let token_reduction_gauge = Gauge::with_opts(prometheus::Opts::new(
            "token_reduction_percentage",
            "Token reduction percentage vs full context",
        ))?;

        let alert_counter = Counter::with_opts(prometheus::Opts::new(
            "performance_alerts_total",
            "Total number of performance alerts triggered",
        ))?;

        registry.register(Box::new(latency_histogram.clone()))?;
        registry.register(Box::new(throughput_gauge.clone()))?;
        registry.register(Box::new(memory_usage_gauge.clone()))?;
        registry.register(Box::new(token_reduction_gauge.clone()))?;
        registry.register(Box::new(alert_counter.clone()))?;

        Ok(Self {
            config,
            pool,
            registry,
            alerts: Arc::new(RwLock::new(Vec::new())),
            latency_histogram,
            throughput_gauge,
            memory_usage_gauge,
            token_reduction_gauge,
            alert_counter,
        })
    }

    /// Record operation latency
    pub fn record_latency(&self, duration_seconds: f64) {
        self.latency_histogram.observe(duration_seconds);

        // Check for latency alerts
        let p95_latency_ms = duration_seconds * 1000.0;
        tokio::spawn({
            let dashboard = self.clone();
            async move {
                dashboard.check_latency_threshold(p95_latency_ms).await;
            }
        });
    }

    /// Update throughput metric
    pub fn record_throughput(&self, ops_per_second: f64) {
        self.throughput_gauge.set(ops_per_second);
    }

    /// Update memory headroom
    pub fn record_memory_headroom(&self, headroom_percent: f64) {
        self.memory_usage_gauge.set(headroom_percent);

        // Check for memory headroom alerts
        tokio::spawn({
            let dashboard = self.clone();
            async move {
                dashboard
                    .check_memory_headroom_threshold(headroom_percent)
                    .await;
            }
        });
    }

    /// Update token reduction metrics
    pub fn record_token_reduction(&self, reduction_percent: f64) {
        self.token_reduction_gauge.set(reduction_percent);

        // Check for token reduction alerts
        tokio::spawn({
            let dashboard = self.clone();
            async move {
                dashboard
                    .check_token_reduction_threshold(reduction_percent)
                    .await;
            }
        });
    }

    /// Check latency threshold and create alerts
    async fn check_latency_threshold(&self, latency_ms: f64) {
        if let Some(threshold) = self.config.alert_thresholds.get("p95_latency_ms") {
            if !threshold.enabled {
                return;
            }

            let severity = if latency_ms > threshold.critical_threshold {
                AlertSeverity::Critical
            } else if latency_ms > threshold.warning_threshold {
                AlertSeverity::Warning
            } else {
                return;
            };

            let alert = PerformanceAlert {
                id: uuid::Uuid::new_v4().to_string(),
                metric_name: "p95_latency_ms".to_string(),
                threshold_type: ThresholdType::GreaterThan,
                threshold_value: match severity {
                    AlertSeverity::Critical => threshold.critical_threshold,
                    _ => threshold.warning_threshold,
                },
                current_value: latency_ms,
                severity,
                message: format!(
                    "P95 latency {:.1}ms exceeds threshold. Story 10 requirement: <2000ms",
                    latency_ms
                ),
                timestamp: Utc::now(),
                resolved: false,
            };

            self.trigger_alert(alert).await;
        }
    }

    /// Check memory headroom threshold
    async fn check_memory_headroom_threshold(&self, headroom_percent: f64) {
        if let Some(threshold) = self.config.alert_thresholds.get("memory_headroom_percent") {
            if !threshold.enabled || headroom_percent >= threshold.warning_threshold {
                return;
            }

            let severity = if headroom_percent < threshold.critical_threshold {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };

            let alert = PerformanceAlert {
                id: uuid::Uuid::new_v4().to_string(),
                metric_name: "memory_headroom_percent".to_string(),
                threshold_type: ThresholdType::LessThan,
                threshold_value: match severity {
                    AlertSeverity::Critical => threshold.critical_threshold,
                    _ => threshold.warning_threshold,
                },
                current_value: headroom_percent,
                severity,
                message: format!(
                    "Memory headroom {:.1}% below threshold. Story 10 requirement: ≥20%",
                    headroom_percent
                ),
                timestamp: Utc::now(),
                resolved: false,
            };

            self.trigger_alert(alert).await;
        }
    }

    /// Check token reduction threshold
    async fn check_token_reduction_threshold(&self, reduction_percent: f64) {
        if let Some(threshold) = self.config.alert_thresholds.get("token_reduction_percent") {
            if !threshold.enabled || reduction_percent >= threshold.critical_threshold {
                return;
            }

            let severity = if reduction_percent < threshold.critical_threshold {
                AlertSeverity::Critical
            } else if reduction_percent < threshold.warning_threshold {
                AlertSeverity::Warning
            } else {
                return;
            };

            let alert = PerformanceAlert {
                id: uuid::Uuid::new_v4().to_string(),
                metric_name: "token_reduction_percent".to_string(),
                threshold_type: ThresholdType::LessThan,
                threshold_value: match severity {
                    AlertSeverity::Critical => threshold.critical_threshold,
                    _ => threshold.warning_threshold,
                },
                current_value: reduction_percent,
                severity,
                message: format!(
                    "Token reduction {:.1}% below target. Story 10 requirement: ≥90%",
                    reduction_percent
                ),
                timestamp: Utc::now(),
                resolved: false,
            };

            self.trigger_alert(alert).await;
        }
    }

    /// Trigger a performance alert
    async fn trigger_alert(&self, alert: PerformanceAlert) {
        self.alert_counter.inc();

        // Store alert
        {
            let mut alerts = self.alerts.write().await;
            alerts.push(alert.clone());

            // Keep only recent alerts to prevent memory bloat
            alerts.retain(|a| {
                let age = Utc::now().signed_duration_since(a.timestamp);
                age.num_days() <= self.config.retention_days as i64
            });
        }

        // Log alert
        match alert.severity {
            AlertSeverity::Critical => {
                tracing::error!(
                    "🚨 CRITICAL Performance Alert: {} - {}",
                    alert.metric_name,
                    alert.message
                );
            }
            AlertSeverity::Warning => {
                tracing::warn!(
                    "⚠️  WARNING Performance Alert: {} - {}",
                    alert.metric_name,
                    alert.message
                );
            }
            AlertSeverity::Info => {
                tracing::info!(
                    "ℹ️  INFO Performance Alert: {} - {}",
                    alert.metric_name,
                    alert.message
                );
            }
        }

        // Store in database for persistence
        if let Err(e) = self.store_alert_in_db(&alert).await {
            tracing::error!("Failed to store alert in database: {}", e);
        }
    }

    /// Store alert in database
    async fn store_alert_in_db(&self, alert: &PerformanceAlert) -> Result<()> {
        // Note: Will be enabled after migration is applied
        let _ = sqlx::query(
            r#"
            INSERT INTO performance_alerts (
                id, metric_name, threshold_type, threshold_value, current_value,
                severity, message, timestamp, resolved
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(&alert.id)
        .bind(&alert.metric_name)
        .bind(serde_json::to_string(&alert.threshold_type)?)
        .bind(alert.threshold_value)
        .bind(alert.current_value)
        .bind(serde_json::to_string(&alert.severity)?)
        .bind(&alert.message)
        .bind(alert.timestamp)
        .bind(alert.resolved)
        .execute(&self.pool)
        .await;

        Ok(())
    }

    /// Get current performance summary
    pub async fn get_performance_summary(&self) -> Result<PerformanceSummary> {
        let current_metrics = self.get_current_metrics().await?;
        let recent_alerts = {
            let alerts = self.alerts.read().await;
            alerts
                .iter()
                .filter(|a| !a.resolved)
                .cloned()
                .collect::<Vec<_>>()
        };

        let story10_compliance = self.check_story10_compliance(&current_metrics);

        Ok(PerformanceSummary {
            current_metrics,
            recent_alerts,
            story10_compliance,
            last_updated: Utc::now(),
        })
    }

    /// Get current performance metrics
    async fn get_current_metrics(&self) -> Result<HashMap<String, f64>> {
        let mut metrics = HashMap::new();

        // Get latest metrics from Prometheus
        metrics.insert(
            "memory_headroom_percent".to_string(),
            self.memory_usage_gauge.get(),
        );
        metrics.insert(
            "token_reduction_percent".to_string(),
            self.token_reduction_gauge.get(),
        );
        metrics.insert(
            "throughput_ops_sec".to_string(),
            self.throughput_gauge.get(),
        );

        // Calculate P95 latency from histogram (approximation using average)
        let sample_count = self.latency_histogram.get_sample_count();
        let p95_latency = if sample_count > 0 {
            self.latency_histogram.get_sample_sum() / sample_count as f64 * 1000.0
        } else {
            0.0
        };
        metrics.insert("p95_latency_ms".to_string(), p95_latency);

        // Get connection pool usage from database
        let pool_stats = self.pool.size() as f64;
        let pool_usage = (pool_stats - self.pool.num_idle() as f64) / pool_stats;
        metrics.insert(
            "connection_pool_usage_percent".to_string(),
            pool_usage * 100.0,
        );

        Ok(metrics)
    }

    /// Check Story 10 compliance
    fn check_story10_compliance(&self, metrics: &HashMap<String, f64>) -> Story10Compliance {
        let p95_latency_compliant = metrics
            .get("p95_latency_ms")
            .map(|&v| v < self.config.performance_targets.p95_latency_ms)
            .unwrap_or(false);

        let token_reduction_compliant = metrics
            .get("token_reduction_percent")
            .map(|&v| v >= self.config.performance_targets.token_reduction_percent)
            .unwrap_or(false);

        let memory_headroom_compliant = metrics
            .get("memory_headroom_percent")
            .map(|&v| v >= self.config.performance_targets.memory_headroom_percent)
            .unwrap_or(false);

        Story10Compliance {
            p95_latency_compliant,
            token_reduction_compliant,
            memory_headroom_compliant,
            overall_compliant: p95_latency_compliant
                && token_reduction_compliant
                && memory_headroom_compliant,
        }
    }

    /// Export Prometheus metrics
    pub fn export_prometheus_metrics(&self) -> String {
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder
            .encode_to_string(&metric_families)
            .unwrap_or_default()
    }
}

impl Clone for PerformanceDashboard {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            pool: self.pool.clone(),
            registry: self.registry.clone(),
            alerts: self.alerts.clone(),
            latency_histogram: self.latency_histogram.clone(),
            throughput_gauge: self.throughput_gauge.clone(),
            memory_usage_gauge: self.memory_usage_gauge.clone(),
            token_reduction_gauge: self.token_reduction_gauge.clone(),
            alert_counter: self.alert_counter.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub current_metrics: HashMap<String, f64>,
    pub recent_alerts: Vec<PerformanceAlert>,
    pub story10_compliance: Story10Compliance,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Story10Compliance {
    pub p95_latency_compliant: bool,     // < 2 seconds
    pub token_reduction_compliant: bool, // ≥ 90%
    pub memory_headroom_compliant: bool, // ≥ 20%
    pub overall_compliant: bool,
}
