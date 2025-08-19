use super::{ComponentHealth, HealthStatus, SystemHealth};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct HealthChecker {
    db_pool: Arc<PgPool>,
    start_time: SystemTime,
    component_thresholds: HealthThresholds,
}

#[derive(Debug, Clone)]
pub struct HealthThresholds {
    pub max_response_time_ms: u64,
    pub max_error_rate: f64,
    pub max_memory_usage_percent: f64,
    pub max_cpu_usage_percent: f64,
    pub max_connection_pool_utilization: f64,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            max_response_time_ms: 1000, // 1 second
            max_error_rate: 0.05,       // 5%
            max_memory_usage_percent: 80.0,
            max_cpu_usage_percent: 90.0,
            max_connection_pool_utilization: 80.0,
        }
    }
}

impl HealthChecker {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self {
            db_pool,
            start_time: SystemTime::now(),
            component_thresholds: HealthThresholds::default(),
        }
    }

    pub fn with_thresholds(mut self, thresholds: HealthThresholds) -> Self {
        self.component_thresholds = thresholds;
        self
    }

    /// Perform comprehensive system health check
    pub async fn check_system_health(&self) -> Result<SystemHealth> {
        let start_check = Instant::now();
        let mut components = HashMap::new();

        // Check database health
        let db_health = self.check_database_health().await;
        components.insert("database".to_string(), db_health);

        // Check memory health
        let memory_health = self.check_memory_health().await;
        components.insert("memory_system".to_string(), memory_health);

        // Check connection pool health
        let pool_health = self.check_connection_pool_health().await;
        components.insert("connection_pool".to_string(), pool_health);

        // Check system resources
        let system_health = self.check_system_resources().await;
        components.insert("system_resources".to_string(), system_health);

        // Determine overall health status
        let overall_status = self.determine_overall_status(&components);

        let uptime = self
            .start_time
            .elapsed()
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();

        let memory_usage = self.get_memory_usage().await.unwrap_or(0);
        let cpu_usage = self.get_cpu_usage().await.unwrap_or(0.0);

        let health = SystemHealth {
            status: overall_status,
            timestamp: Utc::now(),
            components,
            uptime_seconds: uptime,
            memory_usage_bytes: memory_usage,
            cpu_usage_percent: cpu_usage,
        };

        let check_duration = start_check.elapsed().as_millis();
        debug!("System health check completed in {}ms", check_duration);

        Ok(health)
    }

    /// Check database connectivity and performance
    async fn check_database_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut status = HealthStatus::Healthy;
        let mut message = None;
        let mut error_count = 0;

        // Test basic connectivity
        match sqlx::query("SELECT 1 as health_check")
            .fetch_one(self.db_pool.as_ref())
            .await
        {
            Ok(_) => {
                debug!("Database connectivity check passed");
            }
            Err(e) => {
                status = HealthStatus::Unhealthy;
                message = Some(format!("Database connection failed: {e}"));
                error_count += 1;
                error!("Database health check failed: {}", e);
            }
        }

        // Test database performance with a more complex query
        if status == HealthStatus::Healthy {
            match sqlx::query("SELECT COUNT(*) FROM memories WHERE status = 'active'")
                .fetch_one(self.db_pool.as_ref())
                .await
            {
                Ok(_) => {
                    let response_time = start.elapsed().as_millis() as u64;
                    if response_time > self.component_thresholds.max_response_time_ms {
                        status = HealthStatus::Degraded;
                        message = Some(format!("Slow database response: {response_time}ms"));
                        warn!("Database response time degraded: {}ms", response_time);
                    }
                }
                Err(e) => {
                    status = HealthStatus::Degraded;
                    message = Some(format!("Database query performance issue: {e}"));
                    error_count += 1;
                    warn!("Database performance check failed: {}", e);
                }
            }
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        ComponentHealth {
            status,
            message,
            last_checked: Utc::now(),
            response_time_ms: Some(response_time_ms),
            error_count,
        }
    }

    /// Check memory system health
    async fn check_memory_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut status = HealthStatus::Healthy;
        let mut message = None;
        let mut error_count = 0;

        // Check memory tier distribution
        match sqlx::query_as::<_, (String, i64)>(
            "SELECT tier, COUNT(*) FROM memories WHERE status = 'active' GROUP BY tier",
        )
        .fetch_all(self.db_pool.as_ref())
        .await
        {
            Ok(tier_counts) => {
                let total: i64 = tier_counts.iter().map(|(_, count)| count).sum();

                // Check for memory pressure (too many memories in working tier)
                if let Some((_, working_count)) =
                    tier_counts.iter().find(|(tier, _)| tier == "working")
                {
                    let working_ratio = *working_count as f64 / total as f64;
                    if working_ratio > 0.7 {
                        // More than 70% in working tier
                        status = HealthStatus::Degraded;
                        message = Some(format!(
                            "Memory pressure detected: {:.1}% in working tier",
                            working_ratio * 100.0
                        ));
                        warn!(
                            "Memory pressure: {:.1}% of memories in working tier",
                            working_ratio * 100.0
                        );
                    }
                }

                info!(
                    "Memory tier distribution check passed: {} active memories",
                    total
                );
            }
            Err(e) => {
                status = HealthStatus::Degraded;
                message = Some(format!("Memory tier check failed: {e}"));
                error_count += 1;
                warn!("Memory tier health check failed: {}", e);
            }
        }

        // Check for recent migration failures
        match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM migration_history WHERE success = false AND migrated_at > NOW() - INTERVAL '1 hour'"
        )
        .fetch_one(self.db_pool.as_ref())
        .await
        {
            Ok(failure_count) => {
                if failure_count > 10 {
                    status = HealthStatus::Degraded;
                    message = Some(format!("High migration failure rate: {failure_count} failures in last hour"));
                    warn!("High migration failure rate: {} failures in last hour", failure_count);
                }
            }
            Err(e) => {
                warn!("Failed to check migration failures: {}", e);
                error_count += 1;
            }
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        ComponentHealth {
            status,
            message,
            last_checked: Utc::now(),
            response_time_ms: Some(response_time_ms),
            error_count,
        }
    }

    /// Check connection pool health
    async fn check_connection_pool_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut status = HealthStatus::Healthy;
        let mut message = None;

        // Get connection pool statistics
        let pool_size = self.db_pool.size();
        let idle_connections = self.db_pool.num_idle();
        let max_size = 100; // Would get from config in production

        let utilization = if max_size > 0 {
            ((pool_size as usize - idle_connections) as f64 / max_size as f64) * 100.0
        } else {
            0.0
        };

        if utilization > self.component_thresholds.max_connection_pool_utilization {
            status = HealthStatus::Degraded;
            message = Some(format!(
                "High connection pool utilization: {utilization:.1}%"
            ));
            warn!("Connection pool utilization high: {:.1}%", utilization);
        } else if utilization > 90.0 {
            status = HealthStatus::Unhealthy;
            message = Some(format!(
                "Critical connection pool utilization: {utilization:.1}%"
            ));
            error!("Connection pool utilization critical: {:.1}%", utilization);
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "Connection pool health: {}/{} connections used ({:.1}% utilization)",
            pool_size as usize - idle_connections,
            max_size,
            utilization
        );

        ComponentHealth {
            status,
            message,
            last_checked: Utc::now(),
            response_time_ms: Some(response_time_ms),
            error_count: 0,
        }
    }

    /// Check system resource health
    async fn check_system_resources(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut status = HealthStatus::Healthy;
        let mut message = None;

        let memory_usage = self.get_memory_usage().await.unwrap_or(0);
        let cpu_usage = self.get_cpu_usage().await.unwrap_or(0.0);

        // Check memory usage (simplified - would use actual system monitoring in production)
        let memory_usage_mb = memory_usage / (1024 * 1024);
        if memory_usage_mb > 1024 {
            // Simplified threshold
            status = HealthStatus::Degraded;
            message = Some(format!("High memory usage: {memory_usage_mb}MB"));
        }

        // Check CPU usage
        if cpu_usage > self.component_thresholds.max_cpu_usage_percent {
            status = HealthStatus::Degraded;
            let cpu_message = format!("High CPU usage: {cpu_usage:.1}%");
            message = match message {
                Some(existing) => Some(format!("{existing}; {cpu_message}")),
                None => Some(cpu_message),
            };
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        ComponentHealth {
            status,
            message,
            last_checked: Utc::now(),
            response_time_ms: Some(response_time_ms),
            error_count: 0,
        }
    }

    /// Determine overall system health from component health
    fn determine_overall_status(
        &self,
        components: &HashMap<String, ComponentHealth>,
    ) -> HealthStatus {
        let mut has_unhealthy = false;
        let mut has_degraded = false;

        for (component_name, health) in components {
            match health.status {
                HealthStatus::Unhealthy => {
                    has_unhealthy = true;
                    error!(
                        "Component {} is unhealthy: {:?}",
                        component_name, health.message
                    );
                }
                HealthStatus::Degraded => {
                    has_degraded = true;
                    warn!(
                        "Component {} is degraded: {:?}",
                        component_name, health.message
                    );
                }
                HealthStatus::Healthy => {
                    debug!("Component {} is healthy", component_name);
                }
            }
        }

        if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// Get current memory usage (simplified implementation)
    async fn get_memory_usage(&self) -> Result<u64> {
        // In production, would use system monitoring APIs
        // For now, return a placeholder value
        Ok(512 * 1024 * 1024) // 512MB
    }

    /// Get current CPU usage (simplified implementation)
    async fn get_cpu_usage(&self) -> Result<f64> {
        // In production, would use system monitoring APIs
        // For now, return a placeholder value
        Ok(25.0) // 25%
    }
}

/// Simple health check endpoint response
#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleHealthResponse {
    pub status: String,
    pub timestamp: String,
    pub uptime_seconds: u64,
}

impl From<&SystemHealth> for SimpleHealthResponse {
    fn from(health: &SystemHealth) -> Self {
        Self {
            status: match health.status {
                HealthStatus::Healthy => "healthy".to_string(),
                HealthStatus::Degraded => "degraded".to_string(),
                HealthStatus::Unhealthy => "unhealthy".to_string(),
            },
            timestamp: health.timestamp.to_rfc3339(),
            uptime_seconds: health.uptime_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_thresholds_default() {
        let thresholds = HealthThresholds::default();
        assert_eq!(thresholds.max_response_time_ms, 1000);
        assert_eq!(thresholds.max_error_rate, 0.05);
    }

    #[test]
    fn test_simple_health_response_conversion() {
        let health = SystemHealth {
            status: HealthStatus::Healthy,
            timestamp: Utc::now(),
            components: HashMap::new(),
            uptime_seconds: 3600,
            memory_usage_bytes: 1024 * 1024,
            cpu_usage_percent: 25.0,
        };

        let simple: SimpleHealthResponse = (&health).into();
        assert_eq!(simple.status, "healthy");
        assert_eq!(simple.uptime_seconds, 3600);
    }
}
