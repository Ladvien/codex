use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub max_lifetime_seconds: u64,
    // Vector operation specific configurations
    pub statement_timeout_seconds: u64,
    pub enable_prepared_statements: bool,
    pub enable_connection_validation: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "codex_memory".to_string(),
            username: "postgres".to_string(),
            password: "postgres".to_string(),
            // Optimized for high-throughput vector operations (>1000 ops/sec)
            max_connections: 100, // Minimum 100 as per HIGH-004 requirements
            min_connections: 20,  // Higher minimum to reduce connection establishment overhead
            connection_timeout_seconds: 10, // Shorter timeout for faster failure detection
            idle_timeout_seconds: 300, // 5 minutes - prevent resource waste
            max_lifetime_seconds: 3600, // 1 hour - balance recycling vs overhead
            statement_timeout_seconds: 300, // 5 minutes for vector operations
            enable_prepared_statements: true, // Optimize repeated queries
            enable_connection_validation: true, // Ensure connection health
        }
    }
}

#[derive(Debug)]
pub struct ConnectionPool {
    pool: PgPool,
    config: ConnectionConfig,
}

impl ConnectionPool {
    pub async fn new(config: ConnectionConfig) -> Result<Self> {
        let mut connection_string = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );

        // Add vector operation optimizations to connection string
        connection_string.push_str(&format!(
            "?statement_timeout={}s&prepared_statement_cache_queries={}&tcp_keepalives_idle=60&tcp_keepalives_interval=30&tcp_keepalives_count=3",
            config.statement_timeout_seconds,
            if config.enable_prepared_statements { "64" } else { "0" }
        ));

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.connection_timeout_seconds))
            .idle_timeout(Some(Duration::from_secs(config.idle_timeout_seconds)))
            .max_lifetime(Some(Duration::from_secs(config.max_lifetime_seconds)))
            .test_before_acquire(config.enable_connection_validation)
            .connect(&connection_string)
            .await?;

        // Test the connection
        sqlx::query("SELECT 1").fetch_one(&pool).await?;

        info!(
            "Connected to PostgreSQL at {}:{}/{} with {} max connections",
            config.host, config.port, config.database, config.max_connections
        );

        Ok(Self { pool, config })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn check_health(&self) -> Result<bool> {
        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub async fn get_pool_stats(&self) -> PoolStats {
        let size = self.pool.size();
        let idle = self.pool.num_idle() as u32;
        PoolStats {
            size,
            idle,
            max_size: self.config.max_connections,
            active_connections: size - idle,
            waiting_for_connection: 0, // SQLx doesn't expose this directly
            total_connections_created: 0, // Would need custom tracking
            connection_errors: 0,      // Would need custom tracking
        }
    }

    /// Monitor connection pool health and log warnings if thresholds are exceeded
    pub async fn monitor_pool_health(&self) -> Result<PoolHealthStatus> {
        let stats = self.get_pool_stats().await;
        let utilization = stats.utilization_percentage();
        
        let status = if utilization >= 90.0 {
            tracing::error!(
                "CRITICAL: Connection pool utilization at {:.1}% ({}/{} connections active)",
                utilization, stats.active_connections, stats.max_size
            );
            PoolHealthStatus::Critical
        } else if utilization >= 70.0 {
            tracing::warn!(
                "WARNING: Connection pool utilization at {:.1}% ({}/{} connections active)",
                utilization, stats.active_connections, stats.max_size
            );
            PoolHealthStatus::Warning
        } else {
            tracing::debug!(
                "Connection pool healthy: {:.1}% utilization ({}/{} connections active)",
                utilization, stats.active_connections, stats.max_size
            );
            PoolHealthStatus::Healthy
        };

        Ok(status)
    }

    /// Get connection pool metrics for Prometheus/monitoring systems
    pub async fn get_metrics(&self) -> ConnectionPoolMetrics {
        let stats = self.get_pool_stats().await;
        ConnectionPoolMetrics {
            max_connections: stats.max_size,
            active_connections: stats.active_connections,
            idle_connections: stats.idle,
            utilization_percentage: stats.utilization_percentage(),
            health_status: stats.health_status(),
        }
    }

    pub async fn close(&self) {
        self.pool.close().await;
        info!("Connection pool closed");
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub size: u32,
    pub idle: u32,
    pub max_size: u32,
    pub active_connections: u32,
    pub waiting_for_connection: u32,
    pub total_connections_created: u64,
    pub connection_errors: u64,
}

impl PoolStats {
    pub fn utilization_percentage(&self) -> f32 {
        if self.max_size == 0 {
            return 0.0;
        }
        (self.active_connections as f32 / self.max_size as f32) * 100.0
    }

    pub fn is_saturated(&self, threshold: f32) -> bool {
        self.utilization_percentage() >= threshold
    }

    /// Check if pool is at warning level (70% utilization as per requirements)
    pub fn needs_attention(&self) -> bool {
        self.is_saturated(70.0)
    }

    /// Check if pool is critically saturated (90% utilization)
    pub fn is_critically_saturated(&self) -> bool {
        self.is_saturated(90.0)
    }

    /// Get health status message
    pub fn health_status(&self) -> String {
        let utilization = self.utilization_percentage();
        match utilization {
            _ if utilization >= 90.0 => "CRITICAL: Pool >90% utilized".to_string(),
            _ if utilization >= 70.0 => "WARNING: Pool >70% utilized".to_string(),
            _ => format!("HEALTHY: Pool {utilization:.1}% utilized"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PoolHealthStatus {
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ConnectionPoolMetrics {
    pub max_connections: u32,
    pub active_connections: u32,
    pub idle_connections: u32,
    pub utilization_percentage: f32,
    pub health_status: String,
}

pub async fn create_connection_pool(config: ConnectionConfig) -> Result<PgPool> {
    let connection_string = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.username, config.password, config.host, config.port, config.database
    );

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connection_timeout_seconds))
        .idle_timeout(Duration::from_secs(config.idle_timeout_seconds))
        .max_lifetime(Duration::from_secs(config.max_lifetime_seconds))
        .connect(&connection_string)
        .await?;

    Ok(pool)
}

// Optimized pool creation for high-throughput vector operations
pub async fn create_pool(database_url: &str, max_connections: u32) -> Result<PgPool> {
    // Vector workloads require more connections due to CPU-intensive operations
    // that hold connections longer than typical OLTP queries
    let effective_max_connections = std::cmp::max(max_connections, 100); // Minimum 100 for vector workloads
    let min_connections = std::cmp::max(effective_max_connections / 5, 20); // 20% minimum, at least 20

    // Enhanced connection string for vector operations
    let mut enhanced_url = database_url.to_string();
    if !enhanced_url.contains('?') {
        enhanced_url.push('?');
    } else {
        enhanced_url.push('&');
    }
    enhanced_url.push_str("statement_timeout=300s&prepared_statement_cache_queries=64&tcp_keepalives_idle=60&tcp_keepalives_interval=30&tcp_keepalives_count=3");

    let pool = PgPoolOptions::new()
        .max_connections(effective_max_connections)
        .min_connections(min_connections)
        .acquire_timeout(Duration::from_secs(10)) // Longer timeout for vector operations
        .idle_timeout(Some(Duration::from_secs(300))) // 5 minutes
        .max_lifetime(Some(Duration::from_secs(3600))) // 1 hour for vector workloads
        .test_before_acquire(true) // Ensure connection health for vector operations
        .connect(&enhanced_url)
        .await?;

    // Test the connection with vector capability and pgvector extension
    sqlx::query("SELECT vector_dims('[1,2,3]'::vector)")
        .fetch_one(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Vector capability test failed: {}", e))?;

    info!(
        "Connected to PostgreSQL with {} max connections ({} min) - Vector workload optimized",
        effective_max_connections, min_connections
    );
    Ok(pool)
}

pub fn get_pool(pool: &PgPool) -> &PgPool {
    pool
}

/// Enhanced monitoring for vector workload connection pools
pub async fn monitor_vector_pool_health(pool: &PgPool, pool_name: &str) -> Result<bool> {
    let size = pool.size();
    let idle = pool.num_idle() as u32;
    let active = size - idle;
    let utilization = if size > 0 { (active as f32 / size as f32) * 100.0 } else { 0.0 };
    
    // Vector-specific monitoring: check for connection saturation patterns
    let is_healthy = if utilization >= 90.0 {
        tracing::error!(
            "CRITICAL: Vector pool '{}' at {:.1}% utilization ({}/{} active) - connection exhaustion risk",
            pool_name, utilization, active, size
        );
        false
    } else if utilization >= 70.0 {
        tracing::warn!(
            "WARNING: Vector pool '{}' at {:.1}% utilization ({}/{} active) - scaling recommended",
            pool_name, utilization, active, size
        );
        true
    } else {
        tracing::debug!(
            "Vector pool '{}' healthy: {:.1}% utilization ({}/{} active)",
            pool_name, utilization, active, size
        );
        true
    };

    // Test vector capability is still available
    match sqlx::query("SELECT vector_dims('[1,2,3]'::vector)").fetch_one(pool).await {
        Ok(_) => Ok(is_healthy),
        Err(e) => {
            tracing::error!("Vector capability test failed for pool '{}': {}", pool_name, e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_utilization() {
        let stats = PoolStats {
            size: 50,
            idle: 20,
            max_size: 100,
            active_connections: 30, // size - idle = 50 - 20 = 30
            waiting_for_connection: 0,
            total_connections_created: 100,
            connection_errors: 0,
        };

        assert!((stats.utilization_percentage() - 30.0).abs() < 0.01);
        assert!(!stats.is_saturated(70.0));
        assert!(stats.is_saturated(30.0));
        assert!(!stats.needs_attention());
        assert!(!stats.is_critically_saturated());
    }

    #[test]
    fn test_default_config() {
        let config = ConnectionConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.min_connections, 20);
        assert_eq!(config.statement_timeout_seconds, 300);
        assert!(config.enable_prepared_statements);
        assert!(config.enable_connection_validation);
    }

    #[test]
    fn test_pool_stats_health_status() {
        // Test healthy status
        let healthy_stats = PoolStats {
            size: 30,
            idle: 20,
            max_size: 100,
            active_connections: 10,
            waiting_for_connection: 0,
            total_connections_created: 50,
            connection_errors: 0,
        };
        assert!(healthy_stats.health_status().contains("HEALTHY"));

        // Test warning status
        let warning_stats = PoolStats {
            size: 80,
            idle: 5,
            max_size: 100,
            active_connections: 75,
            waiting_for_connection: 0,
            total_connections_created: 150,
            connection_errors: 0,
        };
        assert!(warning_stats.health_status().contains("WARNING"));
        assert!(warning_stats.needs_attention());

        // Test critical status
        let critical_stats = PoolStats {
            size: 95,
            idle: 2,
            max_size: 100,
            active_connections: 93,
            waiting_for_connection: 5,
            total_connections_created: 200,
            connection_errors: 0,
        };
        assert!(critical_stats.health_status().contains("CRITICAL"));
        assert!(critical_stats.is_critically_saturated());
    }
}
