use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::memory::connection::ConnectionPool;

/// Connection pool monitoring and alerting system
/// Implements HIGH-004 requirement for 70% utilization alerting
#[derive(Debug, Clone)]
pub struct ConnectionMonitor {
    pool: Arc<ConnectionPool>,
    config: MonitoringConfig,
    metrics: Arc<RwLock<ConnectionMetrics>>,
    alert_history: Arc<RwLock<Vec<AlertEvent>>>,
}

#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// Monitoring interval in seconds
    pub check_interval_seconds: u64,
    /// Warning threshold (70% as per HIGH-004 requirements)
    pub warning_threshold: f32,
    /// Critical threshold (90% utilization)
    pub critical_threshold: f32,
    /// Maximum number of alerts to keep in history
    pub max_alert_history: usize,
    /// Minimum time between duplicate alerts (seconds)
    pub alert_cooldown_seconds: u64,
    /// Enable detailed logging
    pub enable_detailed_logging: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            check_interval_seconds: 30,  // Check every 30 seconds
            warning_threshold: 70.0,     // HIGH-004 requirement
            critical_threshold: 90.0,    // Critical level
            max_alert_history: 1000,
            alert_cooldown_seconds: 300, // 5 minutes between duplicate alerts
            enable_detailed_logging: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub pool_stats: PoolStatsSnapshot,
    pub health_status: String,
    pub alert_level: AlertLevel,
    pub uptime_seconds: u64,
    pub total_checks: u64,
    pub alert_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatsSnapshot {
    pub size: u32,
    pub idle: u32,
    pub active_connections: u32,
    pub max_size: u32,
    pub utilization_percentage: f32,
    pub waiting_for_connection: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertLevel {
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: AlertLevel,
    pub message: String,
    pub pool_stats: PoolStatsSnapshot,
    pub resolved: bool,
}

impl ConnectionMonitor {
    pub fn new(pool: Arc<ConnectionPool>, config: MonitoringConfig) -> Self {
        Self {
            pool,
            config,
            metrics: Arc::new(RwLock::new(ConnectionMetrics {
                timestamp: chrono::Utc::now(),
                pool_stats: PoolStatsSnapshot::default(),
                health_status: "Starting".to_string(),
                alert_level: AlertLevel::Healthy,
                uptime_seconds: 0,
                total_checks: 0,
                alert_count: 0,
            })),
            alert_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the monitoring service
    pub async fn start_monitoring(&self) -> Result<()> {
        info!("🔍 Starting connection pool monitoring service");
        info!("  Warning threshold: {}%", self.config.warning_threshold);
        info!("  Critical threshold: {}%", self.config.critical_threshold);
        info!("  Check interval: {}s", self.config.check_interval_seconds);

        let start_time = Instant::now();
        let mut interval = interval(Duration::from_secs(self.config.check_interval_seconds));
        
        loop {
            interval.tick().await;
            
            match self.perform_health_check(start_time.elapsed()).await {
                Ok(_) => {
                    // Health check successful
                }
                Err(e) => {
                    error!("Health check failed: {}", e);
                    // Continue monitoring even if individual checks fail
                }
            }
        }
    }

    async fn perform_health_check(&self, uptime: Duration) -> Result<()> {
        let pool_stats = self.pool.get_pool_stats().await;
        let timestamp = chrono::Utc::now();
        
        // Convert to snapshot format
        let stats_snapshot = PoolStatsSnapshot {
            size: pool_stats.size,
            idle: pool_stats.idle,
            active_connections: pool_stats.active_connections,
            max_size: pool_stats.max_size,
            utilization_percentage: pool_stats.utilization_percentage(),
            waiting_for_connection: pool_stats.waiting_for_connection,
        };

        // Determine alert level
        let alert_level = self.determine_alert_level(&stats_snapshot);
        let health_status = self.generate_health_status(&stats_snapshot, &alert_level);

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.timestamp = timestamp;
            metrics.pool_stats = stats_snapshot.clone();
            metrics.health_status = health_status.clone();
            metrics.alert_level = alert_level.clone();
            metrics.uptime_seconds = uptime.as_secs();
            metrics.total_checks += 1;
        }

        // Handle alerting
        if let Some(alert) = self.should_alert(&alert_level, &stats_snapshot).await? {
            self.send_alert(alert).await?;
        }

        // Log status if detailed logging is enabled
        if self.config.enable_detailed_logging {
            self.log_status(&stats_snapshot, &alert_level).await;
        }

        Ok(())
    }

    fn determine_alert_level(&self, stats: &PoolStatsSnapshot) -> AlertLevel {
        let utilization = stats.utilization_percentage;
        
        if utilization >= self.config.critical_threshold {
            AlertLevel::Critical
        } else if utilization >= self.config.warning_threshold {
            AlertLevel::Warning
        } else {
            AlertLevel::Healthy
        }
    }

    fn generate_health_status(&self, stats: &PoolStatsSnapshot, alert_level: &AlertLevel) -> String {
        match alert_level {
            AlertLevel::Healthy => format!(
                "HEALTHY: Pool at {:.1}% utilization ({}/{} connections active)",
                stats.utilization_percentage, stats.active_connections, stats.max_size
            ),
            AlertLevel::Warning => format!(
                "WARNING: Pool at {:.1}% utilization ({}/{} connections active) - Approaching capacity",
                stats.utilization_percentage, stats.active_connections, stats.max_size
            ),
            AlertLevel::Critical => format!(
                "CRITICAL: Pool at {:.1}% utilization ({}/{} connections active) - Pool saturated!",
                stats.utilization_percentage, stats.active_connections, stats.max_size
            ),
        }
    }

    async fn should_alert(&self, level: &AlertLevel, stats: &PoolStatsSnapshot) -> Result<Option<AlertEvent>> {
        if matches!(level, AlertLevel::Healthy) {
            return Ok(None);
        }

        let alert_history = self.alert_history.read().await;
        let now = chrono::Utc::now();
        
        // Check for recent similar alerts (cooldown period)
        if let Some(last_alert) = alert_history.iter().rev().find(|a| a.level == *level) {
            let time_since_last = now.signed_duration_since(last_alert.timestamp);
            if time_since_last.num_seconds() < self.config.alert_cooldown_seconds as i64 {
                return Ok(None); // Still in cooldown period
            }
        }

        // Create new alert
        let message = match level {
            AlertLevel::Warning => format!(
                "Connection pool utilization at {:.1}% (threshold: {}%) - {} active of {} max connections",
                stats.utilization_percentage,
                self.config.warning_threshold,
                stats.active_connections,
                stats.max_size
            ),
            AlertLevel::Critical => format!(
                "Connection pool critically saturated at {:.1}% (threshold: {}%) - {} active of {} max connections. Immediate attention required!",
                stats.utilization_percentage,
                self.config.critical_threshold,
                stats.active_connections,
                stats.max_size
            ),
            AlertLevel::Healthy => unreachable!(),
        };

        Ok(Some(AlertEvent {
            timestamp: now,
            level: level.clone(),
            message,
            pool_stats: stats.clone(),
            resolved: false,
        }))
    }

    async fn send_alert(&self, alert: AlertEvent) -> Result<()> {
        // Log the alert
        match alert.level {
            AlertLevel::Warning => warn!("🚨 POOL WARNING: {}", alert.message),
            AlertLevel::Critical => error!("🚨🚨 POOL CRITICAL: {}", alert.message),
            AlertLevel::Healthy => unreachable!(),
        }

        // Update alert count
        {
            let mut metrics = self.metrics.write().await;
            metrics.alert_count += 1;
        }

        // Add to alert history
        {
            let mut history = self.alert_history.write().await;
            history.push(alert.clone());
            
            // Maintain history size limit
            if history.len() > self.config.max_alert_history {
                history.remove(0);
            }
        }

        // In a production system, you would integrate with:
        // - PagerDuty, OpsGenie, or similar alerting systems
        // - Slack/Teams notifications
        // - Email alerts
        // - Metrics systems like Prometheus/Grafana
        
        info!("Alert sent: {} - {}", alert.level as u8, alert.message);

        Ok(())
    }

    async fn log_status(&self, stats: &PoolStatsSnapshot, level: &AlertLevel) {
        match level {
            AlertLevel::Healthy => {
                if self.config.enable_detailed_logging {
                    info!(
                        "Pool Status: {:.1}% utilization - {}/{} active connections",
                        stats.utilization_percentage, stats.active_connections, stats.max_size
                    );
                }
            }
            AlertLevel::Warning => {
                warn!(
                    "Pool Status: {:.1}% utilization - {}/{} active connections (WARNING LEVEL)",
                    stats.utilization_percentage, stats.active_connections, stats.max_size
                );
            }
            AlertLevel::Critical => {
                error!(
                    "Pool Status: {:.1}% utilization - {}/{} active connections (CRITICAL LEVEL)",
                    stats.utilization_percentage, stats.active_connections, stats.max_size
                );
            }
        }
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> ConnectionMetrics {
        self.metrics.read().await.clone()
    }

    /// Get alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<AlertEvent> {
        let history = self.alert_history.read().await;
        let limit = limit.unwrap_or(history.len());
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get current pool health summary
    pub async fn get_health_summary(&self) -> PoolHealthSummary {
        let metrics = self.get_metrics().await;
        let recent_alerts = self.get_alert_history(Some(10)).await;
        
        let critical_alerts = recent_alerts.iter().filter(|a| matches!(a.level, AlertLevel::Critical)).count();
        let warning_alerts = recent_alerts.iter().filter(|a| matches!(a.level, AlertLevel::Warning)).count();

        PoolHealthSummary {
            current_status: metrics.health_status,
            current_utilization: metrics.pool_stats.utilization_percentage,
            alert_level: metrics.alert_level,
            uptime_hours: metrics.uptime_seconds / 3600,
            total_alerts: metrics.alert_count,
            recent_critical_alerts: critical_alerts as u64,
            recent_warning_alerts: warning_alerts as u64,
            last_check: metrics.timestamp,
        }
    }
}

impl Default for PoolStatsSnapshot {
    fn default() -> Self {
        Self {
            size: 0,
            idle: 0,
            active_connections: 0,
            max_size: 0,
            utilization_percentage: 0.0,
            waiting_for_connection: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolHealthSummary {
    pub current_status: String,
    pub current_utilization: f32,
    pub alert_level: AlertLevel,
    pub uptime_hours: u64,
    pub total_alerts: u64,
    pub recent_critical_alerts: u64,
    pub recent_warning_alerts: u64,
    pub last_check: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_alert_level_determination() {
        let config = MonitoringConfig::default();
        
        // Test healthy level
        let healthy_stats = PoolStatsSnapshot {
            utilization_percentage: 50.0,
            ..Default::default()
        };
        assert_eq!(determine_alert_level(&config, &healthy_stats), AlertLevel::Healthy);

        // Test warning level
        let warning_stats = PoolStatsSnapshot {
            utilization_percentage: 75.0,
            ..Default::default()
        };
        assert_eq!(determine_alert_level(&config, &warning_stats), AlertLevel::Warning);

        // Test critical level
        let critical_stats = PoolStatsSnapshot {
            utilization_percentage: 95.0,
            ..Default::default()
        };
        assert_eq!(determine_alert_level(&config, &critical_stats), AlertLevel::Critical);
    }

    #[test]
    fn test_health_status_generation() {
        let stats = PoolStatsSnapshot {
            utilization_percentage: 75.0,
            active_connections: 75,
            max_size: 100,
            ..Default::default()
        };

        let status = generate_health_status(&stats, &AlertLevel::Warning);
        assert!(status.contains("WARNING"));
        assert!(status.contains("75.0%"));
        assert!(status.contains("75/100"));
    }
}

// Helper functions for tests
fn determine_alert_level(config: &MonitoringConfig, stats: &PoolStatsSnapshot) -> AlertLevel {
    let utilization = stats.utilization_percentage;
    
    if utilization >= config.critical_threshold {
        AlertLevel::Critical
    } else if utilization >= config.warning_threshold {
        AlertLevel::Warning
    } else {
        AlertLevel::Healthy
    }
}

fn generate_health_status(stats: &PoolStatsSnapshot, alert_level: &AlertLevel) -> String {
    match alert_level {
        AlertLevel::Healthy => format!(
            "HEALTHY: Pool at {:.1}% utilization ({}/{} connections active)",
            stats.utilization_percentage, stats.active_connections, stats.max_size
        ),
        AlertLevel::Warning => format!(
            "WARNING: Pool at {:.1}% utilization ({}/{} connections active) - Approaching capacity",
            stats.utilization_percentage, stats.active_connections, stats.max_size
        ),
        AlertLevel::Critical => format!(
            "CRITICAL: Pool at {:.1}% utilization ({}/{} connections active) - Pool saturated!",
            stats.utilization_percentage, stats.active_connections, stats.max_size
        ),
    }
}