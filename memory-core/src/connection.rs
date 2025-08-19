use anyhow::Result;
use deadpool_postgres::{Config, Pool, Runtime};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
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
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "codex_memory".to_string(),
            username: "postgres".to_string(),
            password: "postgres".to_string(),
            max_connections: 100,
            min_connections: 10,
            connection_timeout_seconds: 30,
            idle_timeout_seconds: 600,
            max_lifetime_seconds: 1800,
        }
    }
}

pub struct ConnectionPool {
    pool: PgPool,
    config: ConnectionConfig,
}

impl ConnectionPool {
    pub async fn new(config: ConnectionConfig) -> Result<Self> {
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

        // Test the connection
        sqlx::query("SELECT 1")
            .fetch_one(&pool)
            .await?;

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
        PoolStats {
            size: self.pool.size() as u32,
            idle: self.pool.num_idle() as u32,
            max_size: self.config.max_connections,
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
}

impl PoolStats {
    pub fn utilization_percentage(&self) -> f32 {
        if self.max_size == 0 {
            return 0.0;
        }
        ((self.size - self.idle) as f32 / self.max_size as f32) * 100.0
    }
    
    pub fn is_saturated(&self, threshold: f32) -> bool {
        self.utilization_percentage() >= threshold
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_stats_utilization() {
        let stats = PoolStats {
            size: 50,
            idle: 20,
            max_size: 100,
        };
        
        assert!((stats.utilization_percentage() - 30.0).abs() < 0.01);
        assert!(!stats.is_saturated(70.0));
        assert!(stats.is_saturated(30.0));
    }

    #[test]
    fn test_default_config() {
        let config = ConnectionConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.max_connections, 100);
    }
}